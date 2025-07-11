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
    raise ImportError("Langflow components not available. Install langflow for full functionality.")


# Create the server
server = StepflowLangflowServer(default_protocol_prefix="langflow")

if LANGFLOW_AVAILABLE:
    import msgspec
    
    # Custom encoder/decoder for Langflow types
    def _encode_langflow_types(obj):
        """Custom encoder for Langflow types."""
        if isinstance(obj, (Data, Message)):
            return obj.model_dump()
        elif isinstance(obj, DataFrame):
            if hasattr(obj, 'model_dump'):
                return obj.model_dump()
            else:
                return {"data": obj.to_dict('records') if hasattr(obj, 'to_dict') else str(obj)}
        return obj
    
    def _decode_langflow_types(type_info, obj):
        """Custom decoder for Langflow types."""
        if type_info == Message:
            return Message(**obj)
        elif type_info == Data:
            return Data(**obj)
        # DataFrame is more complex, leave as dict for now
        return obj

# Direct Langflow component wrapper that uses native types
def create_langflow_component_wrapper(component_class, component_name: str):
    """Create a wrapper for Langflow components using native types directly."""
    
    if not LANGFLOW_AVAILABLE:
        @server.component
        def fallback_component(input: Dict[str, Any]) -> Dict[str, Any]:
            return {"error": "Langflow components not available. Please install langflow."}
        return fallback_component
    
    # Get the component's expected input/output types by inspecting its signature
    # TODO: FRAZ - need to look at output_types to find method to run 
    if hasattr(component_class, 'run'):
        run_signature = inspect.signature(component_class.run)
        init_signature = inspect.signature(component_class.__init__)
    else:
        # Fallback for components without run method
        run_signature = None
        init_signature = inspect.signature(component_class.__init__)
    
    @server.component
    async def native_langflow_component(input: Dict[str, Any]) -> Union[Data, Message, DataFrame, Dict[str, Any]]:
        """Direct wrapper for Langflow components using native types."""
        
        try:
            # Initialize the Langflow component with appropriate parameters
            init_params = {}
            
            # TODO: FRAZ - need to look at input_types to find method to run 
            for param_name, param in init_signature.parameters.items():
                if param_name == 'self':
                    continue
                if param_name in input:
                    init_params[param_name] = input[param_name]
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
            # TODO: FRAZ - need to look at output_types to find method to run 
            if hasattr(component_instance, 'run'):
                # Check if run method needs input_value
                if run_signature and 'input_value' in run_signature.parameters:
                    # Find the appropriate input value
                    input_value = input.get('input_value') or input.get('messages') or input.get('data')
                    
                    # Convert input to appropriate Langflow types if needed
                    if isinstance(input_value, list) and input_value and isinstance(input_value[0], dict):
                        # Convert list of message dicts to Message objects
                        if "role" in input_value[0] and "content" in input_value[0]:
                            input_value = [
                                Message(
                                    text=msg.get("content", ""),
                                    sender=msg.get("role", "user"),
                                    sender_name=msg.get("role", "user")
                                ) for msg in input_value
                            ]
                    
                    result = component_instance.run(input_value=input_value)
                else:
                    result = component_instance.run()
            else:
                # Fallback to direct call
                result = component_instance(**input)
            
            # Return the native Langflow result directly
            return result
            
        except Exception as e:
            # Return error as a simple dict
            return {"error": str(e), "component": component_name}
    
    # Set the component name for registration
    native_langflow_component.__name__ = component_name
    return native_langflow_component

# Register actual Langflow components using native types
if LANGFLOW_AVAILABLE:
    # TODO: FRAZ - can we iterate through all components in langflow and register them?
    # Suppose this would be how you can define which components you want to register.

    # Register OpenAI Chat Model - returns Message
    openai_chat = create_langflow_component_wrapper(OpenAIChatModel, "openai_chat")
    
    # Register Prompt Component - returns Data
    prompt_formatter = create_langflow_component_wrapper(PromptComponent, "prompt_formatter")
    
    # Simple component that returns native Data object
    @server.component
    def create_data(input: Dict[str, Any]) -> Data:
        """Create a Data object from input."""
        return Data(
            data=input.get("data", {}),
            text_key=input.get("text_key", "text")
        )
    
    # Simple component that returns native Message object
    @server.component
    def create_message(input: Dict[str, Any]) -> Message:
        """Create a Message object from input."""
        return Message(
            text=input.get("text", ""),
            sender=input.get("sender", "user"),
            sender_name=input.get("sender_name", input.get("sender", "user"))
        )
else:
    raise ImportError("Langflow components not available. Install langflow for full functionality.")

# Simple demo component for testing
@server.component
def echo_component(input: Dict[str, Any]) -> Dict[str, Any]:
    """Simple echo component for testing."""
    return {"echo": input, "message": "Echo component received your data"}

if __name__ == "__main__":
    server.run()