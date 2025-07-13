# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

import sys
import msgspec
import inspect
from typing import Any, Dict, List, Optional, Union
from stepflow_sdk.context import StepflowContext
from langflow.schema.data import Data
from langflow.schema.message import Message
from langflow.schema.dataframe import DataFrame

# Global cache for compiled functions by blob_id
_langflow_function_cache = {}


class LangflowUdfInput(msgspec.Struct):
    """Input structure for Langflow UDF execution."""
    blob_id: str
    input: Union[Data, DataFrame, Message, Dict[str, Any]]


async def langflow_udf(input: LangflowUdfInput, context: StepflowContext) -> Dict[str, Any]:
    """
    Execute Langflow user-defined function (UDF) using cached compiled functions from blobs.
    
    This is similar to the regular UDF but includes Langflow-specific types in the execution environment.

    Args:
        input: Contains blob_id (referencing stored code/schema) and input (langflow data)

    Returns:
        The result of the UDF execution, typically a Langflow type.
    """
    # Check if we have a cached function for this blob_id
    if input.blob_id in _langflow_function_cache:
        print(f"Using cached Langflow function for blob_id: {input.blob_id}", file=sys.stderr)
        compiled_func = _langflow_function_cache[input.blob_id]["function"]
    else:
        print(
            f"Loading and compiling Langflow function for blob_id: {input.blob_id}",
            file=sys.stderr,
        )

        # Get the blob containing the function definition
        try:
            blob_data = await context.get_blob(input.blob_id)
        except Exception as e:
            raise ValueError(f"Failed to retrieve blob {input.blob_id}: {e}")

        # Extract code and schema from blob
        if not isinstance(blob_data, dict):
            raise ValueError(f"Blob {input.blob_id} must contain a dictionary")

        code = blob_data.get("code")
        input_schema = blob_data.get("input_schema")
        function_name = blob_data.get("function_name")

        if not code:
            raise ValueError(f"Blob {input.blob_id} must contain 'code' field")
        if not input_schema:
            raise ValueError(f"Blob {input.blob_id} must contain 'input_schema' field")

        # Compile the function with validation built-in
        compiled_func = _compile_langflow_function(code, function_name, input_schema, context)

        # Cache the compiled function
        _langflow_function_cache[input.blob_id] = {
            "function": compiled_func,
            "input_schema": input_schema,
            "function_name": function_name,
        }

    # Execute the cached function (validation happens inside)
    try:
        if inspect.iscoroutinefunction(compiled_func):
            result = await compiled_func(input.input)
        else:
            result = compiled_func(input.input)
    except Exception as e:
        raise ValueError(f"Langflow function execution failed: {e}")

    print(f"Langflow result: {result}", file=sys.stderr)
    
    # Convert result to msgspec-compatible format
    return _convert_langflow_result(result)


def _convert_langflow_result(result: Any) -> Dict[str, Any]:
    """
    Convert Langflow types to msgspec-compatible dictionaries.
    """
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
        # Recursively convert dict values
        converted = {}
        for key, value in result.items():
            converted[key] = _convert_langflow_result(value)
        return converted
    elif isinstance(result, list):
        # Recursively convert list items
        return [_convert_langflow_result(item) for item in result]
    else:
        # For other types, try to convert to a basic type
        if hasattr(result, 'model_dump'):
            return result.model_dump()
        elif hasattr(result, '__dict__'):
            return result.__dict__
        else:
            return result


def _compile_langflow_function(
    code: str, function_name: str | None, input_schema: dict, context: StepflowContext
):
    """
    Compile a Langflow function from code string and return the callable with validation.
    Includes Langflow-specific types in the execution environment.
    """
    import json
    import jsonschema

    exec_globals = {}
    def validate_input(data):
        """Validate input data against the schema."""
        try:
            jsonschema.validate(data, input_schema)
        except jsonschema.ValidationError as e:
            raise ValueError(f"Langflow input validation failed: {e.message}")
        except jsonschema.SchemaError as e:
            raise ValueError(f"Invalid Langflow schema: {e.message}")

    if function_name is not None:
        # Code contains function definition(s)
        local_scope = {}
        try:
            exec(code, exec_globals, local_scope)
        except Exception as e:
            raise ValueError(f"Langflow code execution failed: {e}")

        # Look for the specified function
        if function_name not in local_scope:
            raise ValueError(f"Langflow function '{function_name}' not found in code")

        func = local_scope[function_name]
        if not callable(func):
            raise ValueError(f"'{function_name}' is not a function")

        sig = inspect.signature(func)
        params = list(sig.parameters)
        if len(params) == 2 and params[1] == "context":
            # Function expects context as second parameter
            async def wrapper(input_data):
                validate_input(input_data)
                if inspect.iscoroutinefunction(func):
                    return await func(input_data, context)
                else:
                    return func(input_data, context)

            return wrapper
        else:
            # Function only expects input data
            def wrapper(input_data):
                validate_input(input_data)
                return func(input_data)

            return wrapper
    else:
        # Code is a function body - wrap it appropriately
        try:
            # Try as expression first (for simple cases)
            wrapped_code = f"lambda input: {code}"
            func = eval(wrapped_code, exec_globals)

            def wrapper(input_data):
                validate_input(input_data)
                return func(input_data)

            return wrapper
        except:
            # If that fails, try as statements in a function body
            try:
                # Properly indent each line of the code
                indented_lines = []
                for line in code.split("\n"):
                    if line.strip():  # Only indent non-empty lines
                        indented_lines.append("    " + line)
                    else:
                        indented_lines.append("")  # Keep empty lines as-is

                func_code = f"""def _temp_langflow_func(input, context):
{chr(10).join(indented_lines)}"""
                local_scope = {}
                exec(func_code, safe_globals, local_scope)
                temp_func = local_scope["_temp_langflow_func"]

                # Wrap to always pass context and validate
                async def wrapper(input_data):
                    validate_input(input_data)
                    if inspect.iscoroutinefunction(temp_func):
                        return await temp_func(input_data, context)
                    else:
                        return temp_func(input_data, context)

                return wrapper
            except Exception as e:
                raise ValueError(f"Langflow code compilation failed: {e}")


def parse_langflow_to_stepflow_workflow(langflow_json: dict) -> List[Dict[str, Any]]:
    """
    Parse a Langflow JSON workflow and convert it to StepFlow workflow steps.
    
    Args:
        langflow_json: The Langflow workflow JSON structure
        
    Returns:
        List of StepFlow workflow steps that create UDF blobs for each component
    """
    data = langflow_json.get('data', {})
    nodes = data.get('nodes', [])
    edges = data.get('edges', [])
    
    # Build dependency graph from edges
    dependencies = _build_dependency_graph(edges)
    
    workflow_steps = []
    
    for node in nodes:
        node_data = node.get('data', {})
        node_id = node_data.get('id')
        node_type = node_data.get('type')
        
        # Skip note nodes and other non-component nodes
        if node_type in ['note', 'noteNode'] or not node_id:
            continue
            
        node_info = node_data.get('node', {})
        
        # Extract component information
        component_info = _extract_component_info(node_data, node_info)
        
        # Generate UDF creation step
        udf_step = _generate_udf_creation_step(node_id, component_info, dependencies.get(node_id, []))
        workflow_steps.append(udf_step)
    
    return workflow_steps


def _build_dependency_graph(edges: List[Dict[str, Any]]) -> Dict[str, List[str]]:
    """Build a dependency graph from Langflow edges."""
    dependencies = {}
    
    for edge in edges:
        source = edge.get('source')
        target = edge.get('target')
        
        if source and target:
            if target not in dependencies:
                dependencies[target] = []
            dependencies[target].append(source)
    
    return dependencies


def _extract_component_info(node_data: Dict[str, Any], node_info: Dict[str, Any]) -> Dict[str, Any]:
    """Extract component information from a Langflow node."""
    template = node_info.get('template', {})
    
    # Extract input schema from template
    input_schema = _generate_input_schema_from_template(template)
    
    # Extract the component type and configuration
    component_type = node_data.get('type', 'unknown')
    display_name = node_info.get('display_name', component_type)
    description = node_info.get('description', '')
    
    return {
        'type': component_type,
        'display_name': display_name,
        'description': description,
        'template': template,
        'input_schema': input_schema,
        'outputs': node_info.get('outputs', [])
    }


def _generate_input_schema_from_template(template: Dict[str, Any]) -> Dict[str, Any]:
    """Generate JSON schema from Langflow template."""
    properties = {}
    required = []
    
    for field_name, field_config in template.items():
        if field_name == '_type' or not isinstance(field_config, dict):
            continue
            
        field_type = field_config.get('type', 'str')
        field_required = field_config.get('required', False)
        field_info = field_config.get('info', '')
        
        # Map Langflow types to JSON schema types
        json_type = _map_langflow_type_to_json_schema(field_type)
        
        property_def = {
            'type': json_type,
            'description': field_info
        }
        
        # Handle specific field configurations
        if field_type == 'dropdown':
            options = field_config.get('options', [])
            if options:
                property_def['enum'] = options
        elif field_type == 'slider':
            range_spec = field_config.get('range_spec', {})
            if 'min' in range_spec:
                property_def['minimum'] = range_spec['min']
            if 'max' in range_spec:
                property_def['maximum'] = range_spec['max']
        
        properties[field_name] = property_def
        
        if field_required:
            required.append(field_name)
    
    return {
        'type': 'object',
        'properties': properties,
        'required': required
    }


def _map_langflow_type_to_json_schema(langflow_type: str) -> str:
    """Map Langflow field types to JSON schema types."""
    type_mapping = {
        'str': 'string',
        'int': 'integer', 
        'float': 'number',
        'bool': 'boolean',
        'list': 'array',
        'dict': 'object',
        'dropdown': 'string',
        'slider': 'number',
        'file': 'string',
        'code': 'string',
        'prompt': 'string',
        'multiline': 'string'
    }
    return type_mapping.get(langflow_type, 'string')


def _generate_udf_creation_step(node_id: str, component_info: Dict[str, Any], dependencies: List[str]) -> Dict[str, Any]:
    """Generate a StepFlow step that creates a UDF blob for this component."""
    component_type = component_info['type']
    
    # Generate code based on component type
    code = _generate_component_code(component_info)
    
    # Create the UDF blob data
    udf_data = {
        'input_schema': component_info['input_schema'],
        'code': code,
        'function_name': None,  # Using lambda/expression format
        'component_type': component_type,
        'display_name': component_info['display_name'],
        'description': component_info['description']
    }
    
    step = {
        'id': f'create_{node_id}_udf',
        'component': 'builtin://put_blob',
        'input': {
            'data': udf_data
        }
    }
    
    # Add dependencies if any
    if dependencies:
        step['depends_on'] = [f'create_{dep}_udf' for dep in dependencies]
    
    return step


def _generate_component_code(component_info: Dict[str, Any]) -> str:
    """Generate Python code for executing this component type."""
    component_type = component_info['type']
    template = component_info['template']
    
    if component_type == 'ChatInput':
        return _generate_chat_input_code(template)
    elif component_type == 'ChatOutput':
        return _generate_chat_output_code(template)
    elif component_type == 'Prompt':
        return _generate_prompt_code(template)
    elif component_type == 'LanguageModelComponent':
        return _generate_language_model_code(template)
    else:
        # Generic component handler
        return _generate_generic_component_code(component_info)


def _generate_chat_input_code(template: Dict[str, Any]) -> str:
    """Generate code for ChatInput component."""
    return """
# ChatInput component - create Message from input
from langflow.schema.message import Message

text = input.get('input_value', input.get('text', ''))
sender = input.get('sender', 'User')
sender_name = input.get('sender_name', 'User')

message = Message(
    text=text,
    sender=sender,
    sender_name=sender_name
)

message
"""


def _generate_chat_output_code(template: Dict[str, Any]) -> str:
    """Generate code for ChatOutput component."""
    return """
# ChatOutput component - format and return message
from langflow.schema.message import Message

if isinstance(input, dict):
    if 'input_value' in input and isinstance(input['input_value'], Message):
        result = input['input_value']
    else:
        # Create message from input data
        text = input.get('input_value', str(input))
        result = Message(
            text=text,
            sender=input.get('sender', 'AI'),
            sender_name=input.get('sender_name', 'AI')
        )
elif isinstance(input, Message):
    result = input
else:
    result = Message(text=str(input), sender='AI', sender_name='AI')

result
"""


def _generate_prompt_code(template: Dict[str, Any]) -> str:
    """Generate code for Prompt component."""
    prompt_template = template.get('template', {}).get('value', 'Answer the user as if you were a GenAI expert.')
    
    code = """
# Prompt component - create formatted prompt
from langflow.schema.message import Message

template = '''""" + prompt_template + """'''

# Simple template formatting - replace {variable} with input values
formatted_text = template
if isinstance(input, dict):
    for key, value in input.items():
        formatted_text = formatted_text.replace('{' + key + '}', str(value))

Message(text=formatted_text)
"""
    return code


def _generate_language_model_code(template: Dict[str, Any]) -> str:
    """Generate code for LanguageModel component."""
    provider = template.get('provider', {}).get('value', 'OpenAI')
    model_name = template.get('model_name', {}).get('value', 'gpt-4o-mini')
    temperature = template.get('temperature', {}).get('value', 0.1)
    
    code = """
# LanguageModel component - simulate model response
from langflow.schema.message import Message
import json

# Extract input text and system message
if isinstance(input, dict):
    input_text = input.get('input_value', {}).get('text', '') if isinstance(input.get('input_value'), dict) else str(input.get('input_value', ''))
    system_message = input.get('system_message', {}).get('text', '') if isinstance(input.get('system_message'), dict) else str(input.get('system_message', ''))
elif hasattr(input, 'text'):
    input_text = input.text
    system_message = ''
else:
    input_text = str(input)
    system_message = ''

# For now, create a mock response
# In a real implementation, this would call the actual model API
response_text = "[Mock """ + provider + " " + model_name + """ Response] System: " + system_message + " User: " + input_text

Message(text=response_text)
"""
    return code


def _generate_generic_component_code(component_info: Dict[str, Any]) -> str:
    """Generate code for generic/unknown components."""
    component_type = component_info['type']
    
    code = """
# Generic component handler for """ + component_type + """
from langflow.schema.message import Message
from langflow.schema.data import Data

# Pass through input data, converting to appropriate Langflow type
if isinstance(input, dict):
    if 'text' in input:
        result = Message(text=input['text'])
    else:
        result = Data(data=input)
elif isinstance(input, str):
    result = Message(text=input)
else:
    result = Data(data={'value': str(input)})

result
"""
    return code