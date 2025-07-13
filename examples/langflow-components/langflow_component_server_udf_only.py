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
from langflow.components.openai.openai_chat_model import OpenAIModelComponent
from langflow.components.processing import PromptComponent
from langflow.schema.data import Data
from langflow.schema.message import Message
from langflow.schema.dataframe import DataFrame
from langflow.custom.custom_component.component import Component

# Create the server
server = StepflowLangflowServer(default_protocol_prefix="langflow")


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
# TODO: FRAZ - can we iterate through all components in langflow and register them?
# Suppose this would be how you can define which components you want to register.

# Register OpenAI Chat Model - returns Message
openai_chat = create_langflow_component_wrapper(OpenAIModelComponent, "openai_chat")

# Register Prompt Component - returns Data
prompt_formatter = create_langflow_component_wrapper(PromptComponent, "prompt_formatter")

# Simple demo component for testing
@server.component
def echo_component(input: Dict[str, Any]) -> Dict[str, Any]:
    """Simple echo component for testing."""
    return {"echo": input, "message": "Echo component received your data"}

# UDF Executor Component
@server.component
async def udf_executor(input: Dict[str, Any]) -> Dict[str, Any]:
    """UDF executor that properly handles Langflow component class instantiation."""
    
    # Extract UDF components from the blob data
    code = input.get('code', '')
    template = input.get('template', {})
    component_type = input.get('component_type', '')
    outputs = input.get('outputs', [])
    selected_output = input.get('selected_output')
    runtime_inputs = input.get('runtime_inputs', {})
    
    try:
        # Execute the Langflow component with proper class handling
        result = await _execute_langflow_component(
            code=code,
            template=template,
            component_type=component_type,
            outputs=outputs,
            selected_output=selected_output,
            runtime_inputs=runtime_inputs
        )
        
        # Convert result to StepFlow format
        return _convert_langflow_result_to_stepflow(result)
        
    except Exception as e:
        return {"error": str(e), "component_type": component_type, "details": str(e)}

async def _execute_langflow_component(
    code: str, 
    template: Dict[str, Any], 
    component_type: str, 
    outputs: List[Dict[str, Any]], 
    selected_output: Optional[str],
    runtime_inputs: Dict[str, Any]
) -> Any:
    """Langflow component executor with proper class instantiation."""
    
    # Temporarily allow all imports
    exec_globals = globals()
    
    # 2. Execute the class definition
    try:
        exec(code, exec_globals)
    except Exception as e:
        raise ValueError(f"Failed to execute Langflow code: {e}")
    
    # 3. Find the component class
    component_class = _find_component_class(exec_globals, component_type)
    if not component_class:
        raise ValueError(f"Could not find component class for {component_type}")
    
    # 4. Determine the execution method from selected_output or outputs metadata
    execution_method = _determine_execution_method(outputs, selected_output)
    
    # 5. Instantiate the component (no parameters needed)
    try:
        component_instance = component_class()
    except Exception as e:
        raise ValueError(f"Failed to instantiate {component_type}: {e}")
    
    # 6. Configure the component by setting attributes from template and runtime inputs
    _configure_component_attributes(component_instance, template, runtime_inputs)
    
    # 7. Execute the specified method
    if not execution_method:
        raise ValueError(f"No execution method specified in outputs for {component_type}")
    
    if not hasattr(component_instance, execution_method):
        raise ValueError(f"Component {component_type} does not have method '{execution_method}'")
    
    try:
        method = getattr(component_instance, execution_method)
        if inspect.iscoroutinefunction(method):
            return await method()
        else:
            return method()
    except Exception as e:
        raise ValueError(f"Failed to execute {execution_method} on {component_type}: {e}")


# def _create_safe_langflow_environment() -> Dict[str, Any]:
#     """Create a safe execution environment with Langflow imports."""
#     exec_globals = {
#         '__builtins__': __builtins__,
#         'os': os,
#         'inspect': inspect,
#         'List': List,
#         'Dict': Dict,
#         'Any': Any,
#         'Optional': Optional,
#         'Union': Union,
#     }
    
#     # Try to import Langflow components dynamically
#     try:
#         from langflow.base.io.chat import ChatComponent
#         from langflow.inputs.inputs import BoolInput
#         from langflow.io import (
#             DropdownInput,
#             FileInput,
#             MessageTextInput,
#             MultilineInput,
#             Output,
#         )
#         from langflow.schema.message import Message
#         from langflow.schema.data import Data
#         from langflow.schema.dataframe import DataFrame
#         from langflow.custom.custom_component.component import Component
        
#         exec_globals.update({
#             'ChatComponent': ChatComponent,
#             'BoolInput': BoolInput,
#             'DropdownInput': DropdownInput,
#             'FileInput': FileInput,
#             'MessageTextInput': MessageTextInput,
#             'MultilineInput': MultilineInput,
#             'Output': Output,
#             'Message': Message,
#             'Data': Data,
#             'DataFrame': DataFrame,
#             'Component': Component,
#         })
#     except ImportError as e:
#         print(f"Warning: Could not import some Langflow components: {e}")
    
#     return exec_globals


def _find_component_class(exec_globals: Dict[str, Any], component_type: str):
    """Find the component class in the execution globals."""
    component_class = exec_globals.get(component_type)
    if component_class and isinstance(component_class, type):
        return component_class
    
    raise ValueError(f"Component class '{component_type}' not found in executed code")


def _determine_execution_method(outputs: List[Dict[str, Any]], selected_output: Optional[str]) -> str:
    """Determine which method to execute based on selected_output or outputs metadata."""
    
    # If selected_output is provided, try to find the corresponding method
    if selected_output:
        for output in outputs:
            if output.get('name') == selected_output:
                method = output.get('method')
                if method:
                    return method
        
        raise ValueError(f"selected_output '{selected_output}' not found in outputs")
    else:
        raise ValueError("No selected_output provided")
    
    # # Fallback to first output's method
    # if not outputs:
    #     raise ValueError("No outputs metadata provided")
    
    # first_output = outputs[0]
    # method = first_output.get('method')
    
    # if not method:
    #     raise ValueError("No method specified in outputs metadata")
    
    # return method


def _configure_component_attributes(component_instance: Any, template: Dict[str, Any], runtime_inputs: Dict[str, Any]) -> None:
    """Configure component by setting attributes from template and runtime inputs."""
    
    # Set attributes from template configuration values
    for key, field_def in template.items():
        if isinstance(field_def, dict) and 'value' in field_def:
            value = field_def['value']
            if value is not None and value != '' and value != 'OPENAI_API_KEY':
                try:
                    setattr(component_instance, key, value)
                except Exception as e:
                    print(f"Warning: Could not set attribute {key}={value}: {e}")
    
    # Set attributes from runtime inputs (from other workflow steps)
    for key, value in runtime_inputs.items():
        if not isinstance(value, dict) or '$from' not in value:
            # Direct value, not a StepFlow reference
            try:
                setattr(component_instance, key, value)
            except Exception as e:
                print(f"Warning: Could not set runtime attribute {key}={value}: {e}")

def _convert_langflow_result_to_stepflow(result: Any) -> Dict[str, Any]:
    """Convert Langflow result to StepFlow format."""
    
    if isinstance(result, Message):
        return {
            "result": {
                "text": result.text,
                "sender": result.sender,
                "sender_name": result.sender_name,
                "type": "Message"
            }
        }
    elif isinstance(result, Data):
        return {
            "result": {
                "data": result.data,
                "text_key": result.text_key,
                "type": "Data"
            }
        }
    elif isinstance(result, DataFrame):
        return {
            "result": {
                "data": result.to_dict('records') if hasattr(result, 'to_dict') else str(result),
                "type": "DataFrame"
            }
        }
    elif isinstance(result, dict):
        return {"result": result}
    else:
        return {"result": str(result)}



if __name__ == "__main__":
    server.run()