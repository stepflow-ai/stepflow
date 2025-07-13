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
from typing import Dict, Any, List, Optional, Union, Type
import os
import inspect
import sys
import json

# Import actual Langflow components and types
from langflow.components.openai.openai_chat_model import OpenAIModelComponent
from langflow.components.processing import PromptComponent
from langflow.schema.data import Data
from langflow.schema.message import Message
from langflow.schema.dataframe import DataFrame
from langflow.custom.custom_component.component import Component

# Create the server
server = StepflowLangflowServer(default_protocol_prefix="langflow")

def _determine_environment_variable(field_name: str, field_value: Any, field_config: dict) -> Optional[str]:
    """Dynamically determine environment variable name from Langflow field metadata."""
    
    # Case 1a: Field value is a template string like "${OPENAI_API_KEY}"
    if (isinstance(field_value, str) and 
        field_value.startswith('${') and field_value.endswith('}')):
        env_var_name = field_value[2:-1]  # Remove ${ and }
        return env_var_name
    
    # Case 1b: Field value is already an environment variable name (Langflow pattern)
    # Examples: "OPENAI_API_KEY", "ASTRA_DB_APPLICATION_TOKEN"
    if (isinstance(field_value, str) and 
        field_value.isupper() and 
        '_' in field_value and 
        any(keyword in field_value for keyword in ['API_KEY', 'TOKEN', 'SECRET', 'ENDPOINT'])):
        return field_value
    
    # Case 2: Non-empty value + SecretStrInput = likely a placeholder that should be replaced
    # This handles cases where Langflow has placeholder values like "openaikey"
    if field_value and field_config.get('_input_type') == 'SecretStrInput':
        field_upper = field_name.upper()
        
        # Smart mapping for common patterns
        if field_name == 'api_key':
            return 'OPENAI_API_KEY'  # Most common default
        elif field_name == 'openai_api_key':
            return 'OPENAI_API_KEY'
        elif field_name == 'token':
            return 'ASTRA_DB_APPLICATION_TOKEN'  # Context-dependent, could be improved
        elif 'openai' in field_name.lower():
            return 'OPENAI_API_KEY'
        elif 'anthropic' in field_name.lower():
            return 'ANTHROPIC_API_KEY'
        elif 'google' in field_name.lower():
            return 'GOOGLE_API_KEY'
        elif 'astra' in field_name.lower():
            return 'ASTRA_DB_APPLICATION_TOKEN'
        else:
            # Generic pattern: convert field name to env var format
            return field_upper

    # Case 3: Empty value + SecretStrInput = derive from field name  
    # This handles cases where Langflow expects environment variable lookup
    if not field_value and field_config.get('_input_type') == 'SecretStrInput':
        # Use same smart mapping logic as Case 2
        field_upper = field_name.upper()
        
        if field_name == 'api_key':
            return 'OPENAI_API_KEY'
        elif field_name == 'openai_api_key':
            return 'OPENAI_API_KEY'
        elif field_name == 'token':
            return 'ASTRA_DB_APPLICATION_TOKEN'
        elif 'openai' in field_name.lower():
            return 'OPENAI_API_KEY'
        elif 'anthropic' in field_name.lower():
            return 'ANTHROPIC_API_KEY'
        elif 'google' in field_name.lower():
            return 'GOOGLE_API_KEY'
        elif 'astra' in field_name.lower():
            return 'ASTRA_DB_APPLICATION_TOKEN'
        else:
            return field_upper
    
    # Case 4: Field has actual credential value or isn't a secret field
    # Return None to indicate we should use the provided value as-is
    return None

# Helper functions for native Langflow type handling
def _serialize_langflow_object(obj: Any) -> Any:
    """Serialize Langflow objects with type metadata for proper deserialization."""
    if isinstance(obj, Message):
        serialized = obj.model_dump(mode='json')
        serialized['__langflow_type__'] = 'Message'
        return serialized
    elif isinstance(obj, Data):
        serialized = obj.model_dump(mode='json')
        serialized['__langflow_type__'] = 'Data'
        return serialized
    elif isinstance(obj, DataFrame):
        # Handle Langflow's custom DataFrame
        try:
            # Use the to_data_list method to preserve Data objects
            data_list = obj.to_data_list() if hasattr(obj, 'to_data_list') else obj.to_dict('records')
            return {
                "__langflow_type__": "DataFrame",
                "data": [item.model_dump(mode='json') if hasattr(item, 'model_dump') else item for item in data_list],
                "text_key": getattr(obj, 'text_key', 'text'),
                "default_value": getattr(obj, 'default_value', '')
            }
        except Exception:
            # Fallback to basic pandas serialization
            return {
                "__langflow_type__": "DataFrame", 
                "data": obj.to_dict('records') if hasattr(obj, 'to_dict') else str(obj)
            }
    elif isinstance(obj, (str, int, float, bool, list, dict, type(None))):
        # Simple serializable types - pass through
        return obj
    else:
        # Complex object that can't be serialized
        # With the new multi-component UDF approach, this should not happen
        # as complex objects are kept in memory within fused components
        raise ValueError(f"Cannot serialize complex object of type {type(obj)}. Consider using component fusion for objects that don't serialize to Message/Data/DataFrame.")

def _create_deferred_execution(obj: Any) -> Dict[str, Any]:
    """Create a deferred execution wrapper for complex objects."""
    obj_type = type(obj)
    module_name = obj_type.__module__
    class_name = obj_type.__name__
    
    # Try to extract constructor arguments from the object
    config = {}
    
    # Common patterns for extracting config from objects
    if hasattr(obj, '__dict__'):
        # Extract simple attributes that look like constructor params
        for attr_name, attr_value in obj.__dict__.items():
            if not attr_name.startswith('_') and isinstance(attr_value, (str, int, float, bool, type(None))):
                config[attr_name] = attr_value
   
    # Generate the recreation code
    imports = f"from {module_name} import {class_name}"
    
    # Build constructor call
    if config:
        config_args = ', '.join([f"{k}={repr(v)}" for k, v in config.items()])
        recreation_code = f"{class_name}({config_args})"
    else:
        recreation_code = f"{class_name}()"
    
    deferred = {
        "__deferred_execution__": {
            "imports": imports,
            "recreation_code": recreation_code,
            "class_name": class_name,
            "module_name": module_name,
            "config": config,
            "original_type": f"{module_name}.{class_name}"
        }
    }
    
    return deferred

def _execute_deferred(deferred_info: Dict[str, Any]) -> Any:
    """Execute deferred code to recreate a complex object."""
    try:
        
        # Create execution environment
        exec_globals = globals().copy()
        exec_locals = {}
        
        # Execute imports
        exec(deferred_info['imports'], exec_globals, exec_locals)
        
        # Execute recreation code
        recreated_obj = eval(deferred_info['recreation_code'], exec_globals, exec_locals)
        
        return recreated_obj
        
    except Exception as e:
        raise RuntimeError(f"Failed to execute deferred object creation: {e}. Imports: {deferred_info['imports']}, Code: {deferred_info['recreation_code']}") from e

def _deserialize_to_langflow_type(obj: Any, expected_type: Optional[Type] = None) -> Any:
    """Deserialize objects back to Langflow types using type metadata."""
    if not isinstance(obj, dict):
        return obj
    
    # Check for explicit type metadata first
    langflow_type = obj.get('__langflow_type__')
    if langflow_type:
        # Remove the type metadata before creating the object
        obj_data = {k: v for k, v in obj.items() if k != '__langflow_type__'}
        
        if langflow_type == 'Message':
            return Message(**obj_data)
        elif langflow_type == 'Data':
            return Data(**obj_data)
        elif langflow_type == 'DataFrame':
            # Reconstruct Langflow's custom DataFrame
            try:
                # Import the custom DataFrame
                from langflow.schema.dataframe import DataFrame as LangflowDataFrame
                from langflow.schema.data import Data
                
                data_list = obj_data.get('data', [])
                text_key = obj_data.get('text_key', 'text')
                default_value = obj_data.get('default_value', '')
                
                # Convert back to Data objects if needed
                if data_list and isinstance(data_list[0], dict):
                    data_objects = [Data(**item) if '__langflow_type__' not in item else Data(**{k: v for k, v in item.items() if k != '__langflow_type__'}) for item in data_list]
                else:
                    data_objects = data_list
                
                return LangflowDataFrame(data=data_objects, text_key=text_key, default_value=default_value)
            except Exception:
                # Failed to reconstruct DataFrame, use raw data
                return obj_data.get('data', obj_data)
    
    # If no type metadata and no expected type, this is an error
    if expected_type is None:
        raise ValueError(f"Cannot deserialize object to Langflow type: missing __langflow_type__ metadata and no expected_type provided. Object keys: {list(obj.keys())}")
    
    # Use the explicitly provided expected type
    if expected_type == Message:
        return Message(**obj)
    elif expected_type == Data:
        return Data(**obj)
    
    return obj

# Simple demo component for testing
@server.component
def echo_component(input: Dict[str, Any]) -> Dict[str, Any]:
    """Simple echo component for testing."""
    return {"echo": input, "message": "Echo component received your data"}

# UDF Executor Component
@server.component
async def udf_executor(input: Dict[str, Any], context: StepflowContext) -> Dict[str, Any]:
    """UDF executor that properly handles Langflow component class instantiation."""
    
    
    # Get blob_id and fetch the UDF data
    blob_id = input.get('blob_id')
    if not blob_id:
        return {"error": "No blob_id provided", "details": "UDF executor requires blob_id"}
    
    try:
        blob_data = await context.get_blob(blob_id)
    except Exception as e:
        return {"error": f"Failed to fetch blob {blob_id}", "details": str(e)}
    
    # Check if this is a multi-component UDF (fused components)
    if 'components' in blob_data and 'execution_chain' in blob_data:
        return await _execute_multi_component_udf(blob_data, input, context)
    
    # Extract UDF components from the blob data (single component)
    code = blob_data.get('code', '')
    template = blob_data.get('template', {})
    component_type = blob_data.get('component_type', '')
    outputs = blob_data.get('outputs', [])
    selected_output = blob_data.get('selected_output')
    
    # Get runtime inputs from the input (these come from other workflow steps)
    runtime_inputs = input.get('input', {})
    
    
    
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
        
        # Serialize result for StepFlow protocol, but preserve type info for deserialization
        serialized_result = _serialize_langflow_object(result)
        return {"result": serialized_result}
        
    except Exception as e:
        # FAIL FAST: Re-raise the exception instead of returning error objects
        # This prevents error propagation through the workflow and fails immediately
        raise RuntimeError(f"Component {component_type} failed: {str(e)}") from e

async def _execute_multi_component_udf(blob_data: Dict[str, Any], input: Dict[str, Any], context: StepflowContext) -> Dict[str, Any]:
    """Execute a multi-component UDF where components are chained and complex objects stay in memory."""
    
    components = blob_data.get('components', [])
    execution_chain = blob_data.get('execution_chain', [])
    runtime_inputs = input.get('input', {})
    internal_edge_mappings = blob_data.get('internal_edge_mappings', {})
    
    if not components or not execution_chain:
        raise ValueError("Multi-component UDF requires both 'components' and 'execution_chain'")
    
    # Create a shared execution environment for all components
    exec_globals = globals().copy()
    exec_globals['os'] = os
    
    # Execute all component code in the shared environment
    component_classes = {}
    for component_info in components:
        code = component_info.get('code', '')
        component_type = component_info.get('component_type', '')
        
        if not code or not component_type:
            raise ValueError(f"Component missing code or component_type: {component_info}")
        
        # Execute the component code
        try:
            exec(code, exec_globals)
        except Exception as e:
            raise ValueError(f"Failed to execute code for {component_type}: {e}")
        
        # Find and store the component class
        component_class = _find_component_class(exec_globals, component_type)
        if not component_class:
            raise ValueError(f"Could not find component class for {component_type}")
        
        component_classes[component_type] = component_class
    
    # Execute the component chain, keeping complex objects in memory
    previous_result = None
    
    for i, component_type in enumerate(execution_chain):
        # Find the component info for this step
        component_info = None
        for comp in components:
            if comp.get('component_type') == component_type:
                component_info = comp
                break
        
        if not component_info:
            raise ValueError(f"Component {component_type} not found in components list")
        
        # Get the component class
        component_class = component_classes[component_type]
        
        # Instantiate the component
        try:
            component_instance = component_class()
        except Exception as e:
            raise ValueError(f"Failed to instantiate {component_type}: {e}")
        
        # Configure the component
        template = component_info.get('template', {})
        component_parameters = {}
        
        # Process template parameters with environment variable resolution
        for key, field_def in template.items():
            if isinstance(field_def, dict) and 'value' in field_def:
                value = field_def['value']
                
                # Dynamic credential handling
                env_var = _determine_environment_variable(key, value, field_def)
                if env_var:
                    actual_value = os.getenv(env_var)
                    if actual_value:
                        value = actual_value
                    elif field_def.get('_input_type') == 'SecretStrInput':
                        print(f"⚠️  Environment variable {env_var} not found for {key}", file=sys.stderr)
                
                component_parameters[key] = value
        
        # Add runtime inputs for any component that needs them (not just the first)
        # This allows multi-input fusion chains where multiple components need external data
        for key, value in runtime_inputs.items():
            # Check if this component has a template field that matches this runtime input
            if key in template:
                # Handle serialized Langflow objects
                if isinstance(value, dict) and '__langflow_type__' in value:
                    actual_value = _deserialize_to_langflow_type(value)
                    component_parameters[key] = actual_value
                elif isinstance(value, str) and key in ['input_value', 'input_data']:
                    # Wrap strings in Data objects for input fields
                    try:
                        wrapped_value = Data(data={"text": value})
                        component_parameters[key] = wrapped_value
                    except Exception:
                        component_parameters[key] = value
                else:
                    component_parameters[key] = value
        
        # For subsequent components, use the previous result as input based on actual edge mappings
        if i > 0 and previous_result is not None:
            # Find the current component ID from the fusion chain
            current_component_id = blob_data.get('fusion_chain_ids', [])[i] if i < len(blob_data.get('fusion_chain_ids', [])) else None
            previous_component_id = blob_data.get('fusion_chain_ids', [])[i-1] if i > 0 and i-1 < len(blob_data.get('fusion_chain_ids', [])) else None
            
            if current_component_id and current_component_id in internal_edge_mappings:
                # Use actual edge mappings to determine the correct input field
                for edge in internal_edge_mappings[current_component_id]:
                    if edge['source'] == previous_component_id:
                        input_field = edge['target_field']
                        component_parameters[input_field] = previous_result
                        break
        
        # Configure the component using Langflow's method
        if hasattr(component_instance, 'set_attributes'):
            component_instance._parameters = component_parameters
            component_instance.set_attributes(component_parameters)
        else:
            print(f"⚠️  Component {component_type} doesn't have set_attributes method", file=sys.stderr)
        
        # Execute the component
        outputs = component_info.get('outputs', [])
        selected_output = component_info.get('selected_output')
        execution_method = _determine_execution_method(outputs, selected_output)
        
        if not execution_method:
            raise ValueError(f"No execution method specified for {component_type}")
        
        if not hasattr(component_instance, execution_method):
            available_methods = [m for m in dir(component_instance) if not m.startswith('_')]
            raise ValueError(f"Component {component_type} does not have method '{execution_method}'. Available: {available_methods}")
        
        try:
            method = getattr(component_instance, execution_method)
            
            if inspect.iscoroutinefunction(method):
                result = await method()
            else:
                # Handle sync methods that might have asyncio issues
                result = await _execute_sync_method_safely(method, component_type)
            
            # Store the result for the next component (keep complex objects in memory)
            previous_result = result
            
        except Exception as e:
            raise RuntimeError(f"Failed to execute {execution_method} on {component_type}: {e}") from e
    
    # Return the final result, serialized for StepFlow protocol
    serialized_result = _serialize_langflow_object(previous_result)
    return {"result": serialized_result}

def _determine_input_field_for_component(component_type: str, template: Dict[str, Any]) -> Optional[str]:
    """Determine the input field name for a component based on its type and template."""
    
    # Common input field patterns
    common_inputs = ['input_value', 'input_data', 'documents', 'embeddings', 'texts']
    
    # Look for input fields in the template
    for field_name in template.keys():
        if any(input_pattern in field_name.lower() for input_pattern in ['input', 'document', 'text', 'embedding']):
            return field_name
    
    # Fallback to common patterns
    for common_input in common_inputs:
        if common_input in template:
            return common_input
    
    # Return the first non-config field as fallback
    for field_name, field_config in template.items():
        if isinstance(field_config, dict) and not field_name.startswith('_'):
            # Skip obvious config fields
            if field_name not in ['api_key', 'model_name', 'temperature', 'max_tokens']:
                return field_name
    
    return None

async def _execute_langflow_component(
    code: str, 
    template: Dict[str, Any], 
    component_type: str, 
    outputs: List[Dict[str, Any]], 
    selected_output: Optional[str],
    runtime_inputs: Dict[str, Any]
) -> Any:
    """Langflow component executor with proper class instantiation."""
    
    
    # 1. Set up execution environment 
    exec_globals = globals().copy()
    
    # Ensure environment variables are available to the UDF execution
    # This is crucial for API keys and other configuration
    exec_globals['os'] = os
    
    
    # 2. Execute the class definition
    try:
        exec(code, exec_globals)
    except Exception as e:
        raise ValueError(f"Failed to execute Langflow code: {e}")
    
    # 3. Find the component class
    component_class = _find_component_class(exec_globals, component_type)
    if not component_class:
        available_classes = [k for k, v in exec_globals.items() if isinstance(v, type)]
        print(f"❌ EXECUTE_COMPONENT: Component class '{component_type}' not found. Available: {available_classes}", file=sys.stderr)
        raise ValueError(f"Could not find component class for {component_type}")
    
    # 4. Determine the execution method from selected_output or outputs metadata
    execution_method = _determine_execution_method(outputs, selected_output)
    
    # 5. Instantiate the component (no parameters needed)
    try:
        component_instance = component_class()
    except Exception as e:
        raise ValueError(f"Failed to instantiate {component_type}: {e}")
    
    # 6. Configure the component using Langflow's official method
    component_parameters = {}
    
    # Combine template and runtime inputs into parameters dict
    for key, field_def in template.items():
        if isinstance(field_def, dict) and 'value' in field_def:
            value = field_def['value']
            
            # Dynamic credential handling based on Langflow field metadata
            env_var = _determine_environment_variable(key, value, field_def)
            if env_var:
                # This field should use an environment variable
                actual_value = os.getenv(env_var)
                if actual_value:
                    value = actual_value
                elif field_def.get('_input_type') == 'SecretStrInput':
                    print(f"⚠️  Environment variable {env_var} not found for {key}", file=sys.stderr)
            # If env_var is None, use the provided value as-is (no environment variable substitution needed)
            
            component_parameters[key] = value
    
    # Add runtime inputs (these override template values)
    for key, value in runtime_inputs.items():
        # Check if this is a deferred execution object
        if isinstance(value, dict) and '__deferred_execution__' in value:
            # Execute deferred code to recreate the complex object
            actual_value = _execute_deferred(value['__deferred_execution__'])
            component_parameters[key] = actual_value
        # Check if this is a serialized Langflow object that needs to be deserialized
        elif isinstance(value, dict) and '__langflow_type__' in value:
            # Serialized Langflow object - deserialize it
            actual_value = _deserialize_to_langflow_type(value)
            component_parameters[key] = actual_value
        elif isinstance(value, str) and key in ['input_value', 'input_data']:
            # Special case: if we receive a plain string for input fields, wrap it in a Data object
            # This handles cases where upstream components return strings but downstream expects Data
            try:
                wrapped_value = Data(data={"text": value})
                component_parameters[key] = wrapped_value
            except Exception:
                # If Data creation fails, fall back to original value
                component_parameters[key] = value
        else:
            # Direct value - use as-is
            component_parameters[key] = value
    
    
        
    
    # Use Langflow's official configuration method
    if hasattr(component_instance, 'set_attributes'):
        component_instance._parameters = component_parameters
        component_instance.set_attributes(component_parameters)
    else:
        # Fallback - log warning but continue (most Langflow components should have set_attributes)
        print(f"⚠️  Component doesn't have set_attributes method", file=sys.stderr)
    
    # 7. Execute the specified method
    if not execution_method:
        raise ValueError(f"No execution method specified in outputs for {component_type}")
    
    if not hasattr(component_instance, execution_method):
        available_methods = [m for m in dir(component_instance) if not m.startswith('_')]
        raise ValueError(f"Component {component_type} does not have method '{execution_method}'. Available: {available_methods}")
    
    try:
        method = getattr(component_instance, execution_method)
        
        if inspect.iscoroutinefunction(method):
            result = await method()
        else:
            # Handle sync methods that might have asyncio issues
            result = await _execute_sync_method_safely(method, component_type)
            
        return result
    except Exception as e:
        raise RuntimeError(f"Failed to execute {execution_method} on {component_type}: {e}") from e


async def _execute_sync_method_safely(method, component_type: str):
    """Execute a sync method safely, handling asyncio.run() conflicts."""
    import asyncio
    import concurrent.futures
    
    # For components that might have asyncio issues, run them in a thread pool
    if component_type in ['URLComponent', 'RecursiveUrlLoader']:
        loop = asyncio.get_event_loop()
        with concurrent.futures.ThreadPoolExecutor() as executor:
            future = executor.submit(method)
            result = await loop.run_in_executor(None, lambda: future.result())
            return result
    else:
        # For other components, execute normally
        return method()


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


def _find_component_class(exec_globals: Dict[str, Any], component_type: str) -> Type:
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
            output_name = output.get('name')
            output_method = output.get('method')
            
            if output_name == selected_output:
                if output_method:
                    return output_method
                else:
                    raise ValueError(f"Output '{selected_output}' found but no method specified")
        
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


# Removed manual attribute configuration - using Langflow's native set_attributes method

# Removed manual conversion functions - using native Langflow types throughout



if __name__ == "__main__":
    server.run()