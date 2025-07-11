#!/usr/bin/env python3
"""
Langflow component server with generic wrapper for native Langflow components.

This demonstrates how to create a generic wrapper that:
1. Uses native Langflow types (Data, Message, DataFrame)
2. Dynamically wraps any Langflow component
3. Automatically handles type conversion and schema introspection
"""

from stepflow_langflow import StepflowLangflowServer
from stepflow_sdk import StepflowContext
import msgspec
from typing import Dict, Any, List, Optional, Union
import os
import inspect

# Import actual Langflow components and types
try:
    from langflow.components.openai import OpenAIChatModel
    from langflow.components.prompts import PromptComponent
    from langflow.schema.data import Data
    from langflow.schema.message import Message
    from langflow.schema.dataframe import DataFrame
    from langflow.custom import Component
    LANGFLOW_AVAILABLE = True
except ImportError:
    LANGFLOW_AVAILABLE = False
    print("Warning: Langflow components not available. Install langflow for full functionality.")
    # Mock classes for fallback
    class Data:
        def __init__(self, data=None, text_key="text"):
            self.data = data or {}
            self.text_key = text_key
        def model_dump(self):
            return self.data
    class Message(Data):
        def __init__(self, text="", sender="user", sender_name="user", **kwargs):
            super().__init__()
            self.text = text
            self.sender = sender
            self.sender_name = sender_name
    class DataFrame:
        pass
    class Component:
        pass

# Create the server
server = StepflowLangflowServer(default_protocol_prefix="langflow")

# Generic input/output types for Langflow components
class LangflowInput(msgspec.Struct):
    """Generic input for Langflow components using native types."""
    data: Dict[str, Any] = {}  # Raw component parameters
    context: Optional[Dict[str, Any]] = None  # Additional context

class LangflowOutput(msgspec.Struct):
    """Generic output for Langflow components using native types."""
    result: Dict[str, Any]  # Serialized Langflow result
    type: str  # Type of result (Data, Message, DataFrame, etc.)

# Utility functions for type conversion
def _convert_langflow_result(result: Any) -> Dict[str, Any]:
    """Convert Langflow types to msgspec-compatible dictionaries."""
    if isinstance(result, (Data, Message)):
        # Pydantic models with model_dump()
        return result.model_dump()
    elif isinstance(result, DataFrame):
        # DataFrame might not have model_dump, convert to dict representation
        try:
            if hasattr(result, 'model_dump'):
                return result.model_dump()
            else:
                # Convert DataFrame to dict
                return {
                    "data": result.to_dict('records') if hasattr(result, 'to_dict') else str(result),
                    "type": "dataframe"
                }
        except Exception:
            return {"data": str(result), "type": "dataframe"}
    elif isinstance(result, dict):
        return result
    else:
        return {"data": str(result), "type": "unknown"}

def _convert_to_langflow_types(data: Dict[str, Any]) -> Dict[str, Any]:
    """Convert input data to appropriate Langflow types."""
    converted = {}
    
    for key, value in data.items():
        if isinstance(value, dict):
            # Check if this looks like a Message or Data object
            if "text" in value and "sender" in value:
                converted[key] = Message(
                    text=value.get("text", ""),
                    sender=value.get("sender", "user"),
                    sender_name=value.get("sender_name", value.get("sender", "user"))
                )
            elif "data" in value or "text_key" in value:
                converted[key] = Data(
                    data=value.get("data", value),
                    text_key=value.get("text_key", "text")
                )
            else:
                converted[key] = value
        elif isinstance(value, list) and value and isinstance(value[0], dict):
            # Convert list of message dicts to Message objects
            if "role" in value[0] and "content" in value[0]:
                converted[key] = [
                    Message(
                        text=msg.get("content", ""),
                        sender=msg.get("role", "user"),
                        sender_name=msg.get("role", "user")
                    ) for msg in value
                ]
            else:
                converted[key] = value
        else:
            converted[key] = value
    
    return converted

def _get_result_type(result: Any) -> str:
    """Get the string type name for a Langflow result."""
    if isinstance(result, Message):
        return "Message"
    elif isinstance(result, Data):
        return "Data"
    elif isinstance(result, DataFrame):
        return "DataFrame"
    elif isinstance(result, dict):
        return "dict"
    else:
        return "unknown"

# Generic Langflow component wrapper
def create_langflow_component_wrapper(component_class, component_name: str):
    """Create a generic wrapper for any Langflow component."""
    
    if not LANGFLOW_AVAILABLE:
        @server.component
        def fallback_component(input: LangflowInput) -> LangflowOutput:
            return LangflowOutput(
                result={"error": "Langflow components not available. Please install langflow."},
                type="error"
            )
        return fallback_component
    
    @server.component
    async def generic_langflow_component(input: LangflowInput) -> LangflowOutput:
        """Generic wrapper for Langflow components using native types."""
        
        try:
            # Convert input data to appropriate Langflow types
            converted_data = _convert_to_langflow_types(input.data)
            
            # Initialize the Langflow component
            # Get the component's __init__ signature to pass appropriate parameters
            init_signature = inspect.signature(component_class.__init__)
            init_params = {}
            
            for param_name, param in init_signature.parameters.items():
                if param_name == 'self':
                    continue
                if param_name in converted_data:
                    init_params[param_name] = converted_data[param_name]
                elif param.default != inspect.Parameter.empty:
                    # Use default value if available
                    continue
                else:
                    # Required parameter not provided
                    if param_name == 'api_key':
                        # Special handling for API keys
                        init_params[param_name] = os.getenv("OPENAI_API_KEY")
                    
            # Create component instance
            component_instance = component_class(**init_params)
            
            # Execute the component
            # Try different execution methods based on the component
            if hasattr(component_instance, 'run'):
                # Check if run method needs input_value
                run_signature = inspect.signature(component_instance.run)
                if 'input_value' in run_signature.parameters:
                    # Find the appropriate input value
                    input_value = converted_data.get('input_value') or converted_data.get('messages') or converted_data.get('data')
                    result = component_instance.run(input_value=input_value)
                else:
                    result = component_instance.run()
            else:
                # Fallback to direct call
                result = component_instance(**converted_data)
            
            # Convert result to StepFlow format
            converted_result = _convert_langflow_result(result)
            result_type = _get_result_type(result)
            
            return LangflowOutput(
                result=converted_result,
                type=result_type
            )
            
        except Exception as e:
            return LangflowOutput(
                result={"error": str(e), "component": component_name},
                type="error"
            )
    
    # Set the component name for registration
    generic_langflow_component.__name__ = component_name
    return generic_langflow_component

# Register actual Langflow components using the generic wrapper
if LANGFLOW_AVAILABLE:
    # Register OpenAI Chat Model
    openai_chat = create_langflow_component_wrapper(OpenAIChatModel, "openai_chat")
    
    # Register Prompt Component
    prompt_formatter = create_langflow_component_wrapper(PromptComponent, "prompt_formatter")
    
    # You can easily add more components here:
    # text_splitter = create_langflow_component_wrapper(TextSplitterComponent, "text_splitter")
    # embeddings = create_langflow_component_wrapper(EmbeddingsComponent, "embeddings")
else:
    # Fallback components
    @server.component
    def openai_chat(input: LangflowInput) -> LangflowOutput:
        return LangflowOutput(
            result={"error": "Langflow not available"},
            type="error"
        )
    
    @server.component
    def prompt_formatter(input: LangflowInput) -> LangflowOutput:
        # Simple template replacement fallback
        template = input.data.get("template", "")
        variables = input.data.get("variables", {})
        
        formatted = template
        for key, value in variables.items():
            formatted = formatted.replace(f"{{{key}}}", str(value))
        
        return LangflowOutput(
            result={"formatted_prompt": formatted},
            type="dict"
        )

# Simple demo component for testing
@server.component
def echo_component(input: LangflowInput) -> LangflowOutput:
    """Simple echo component for testing the generic wrapper."""
    return LangflowOutput(
        result={"echo": input.data, "message": "Echo component received your data"},
        type="dict"
    )

if __name__ == "__main__":
    server.run()