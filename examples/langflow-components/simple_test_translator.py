#!/usr/bin/env python3
"""
Simple test script for the new Langflow to StepFlow UDF translator.
This is a standalone version that doesn't require full module imports.
"""

import json
import yaml
from pathlib import Path
from typing import Dict, Any, List


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


def _generate_component_code(component_info: Dict[str, Any]) -> str:
    """Generate Python code for executing this component type."""
    component_type = component_info['type']
    template = component_info['template']
    
    if component_type == 'ChatInput':
        return """
# ChatInput component - create Message from input
text = input.get('input_value', input.get('text', ''))
sender = input.get('sender', 'User')
sender_name = input.get('sender_name', 'User')

result = {
    'text': text,
    'sender': sender, 
    'sender_name': sender_name,
    'type': 'Message'
}
result
"""
    elif component_type == 'ChatOutput':
        return """
# ChatOutput component - format and return message
if isinstance(input, dict):
    if 'input_value' in input:
        result = input['input_value']
    else:
        result = {
            'text': input.get('input_value', str(input)),
            'sender': input.get('sender', 'AI'),
            'sender_name': input.get('sender_name', 'AI'),
            'type': 'Message'
        }
else:
    result = {'text': str(input), 'sender': 'AI', 'sender_name': 'AI', 'type': 'Message'}

result
"""
    elif component_type == 'Prompt':
        prompt_template = template.get('template', {}).get('value', 'Answer the user as if you were a GenAI expert.')
        code = """
# Prompt component - create formatted prompt
template = '''""" + prompt_template + """'''

# Simple template formatting - replace {variable} with input values
formatted_text = template
if isinstance(input, dict):
    for key, value in input.items():
        formatted_text = formatted_text.replace('{' + key + '}', str(value))

result = {'text': formatted_text, 'type': 'Message'}
result
"""
        return code
    elif component_type == 'LanguageModelComponent':
        provider = template.get('provider', {}).get('value', 'OpenAI')
        model_name = template.get('model_name', {}).get('value', 'gpt-4o-mini')
        
        code = """
# LanguageModel component - simulate model response
# Extract input text and system message
if isinstance(input, dict):
    input_text = input.get('input_value', {}).get('text', '') if isinstance(input.get('input_value'), dict) else str(input.get('input_value', ''))
    system_message = input.get('system_message', {}).get('text', '') if isinstance(input.get('system_message'), dict) else str(input.get('system_message', ''))
elif hasattr(input, 'get'):
    input_text = str(input.get('text', input))
    system_message = ''
else:
    input_text = str(input)
    system_message = ''

# For now, create a mock response
response_text = "[Mock """ + provider + " " + model_name + """ Response] System: " + system_message + " User: " + input_text

result = {'text': response_text, 'type': 'Message'}
result
"""
        return code
    else:
        # Generic component handler
        code = """
# Generic component handler for """ + component_type + """
# Pass through input data, converting to appropriate format
if isinstance(input, dict):
    if 'text' in input:
        result = {'text': input['text'], 'type': 'Message'}
    else:
        result = {'data': input, 'type': 'Data'}
elif isinstance(input, str):
    result = {'text': input, 'type': 'Message'}
else:
    result = {'data': {'value': str(input)}, 'type': 'Data'}

result
"""
        return code


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


def test_langflow_translation():
    """Test the new Langflow translation functionality."""
    
    # Load the example Langflow JSON
    langflow_file = Path(__file__).parent / "ExampleLangflow.json"
    
    if not langflow_file.exists():
        print(f"Error: {langflow_file} not found")
        return 1
    
    print(f"Loading Langflow JSON from: {langflow_file}")
    with open(langflow_file, 'r') as f:
        langflow_data = json.load(f)
    
    print(f"Loaded workflow: {langflow_data.get('name', 'Unnamed')}")
    print(f"Description: {langflow_data.get('description', 'No description')}")
    
    # Parse the Langflow JSON into StepFlow workflow steps
    print("\n=== Converting to StepFlow UDF steps ===")
    try:
        workflow_steps = parse_langflow_to_stepflow_workflow(langflow_data)
        
        print(f"Generated {len(workflow_steps)} UDF creation steps:")
        
        for i, step in enumerate(workflow_steps, 1):
            step_id = step.get('id', 'unknown')
            udf_data = step.get('input', {}).get('data', {})
            component_type = udf_data.get('component_type', 'unknown')
            
            print(f"\n{i}. Step ID: {step_id}")
            print(f"   Component Type: {component_type}")
            print(f"   Display Name: {udf_data.get('display_name', 'N/A')}")
            print(f"   Has Input Schema: {bool(udf_data.get('input_schema'))}")
            print(f"   Code Lines: {len(udf_data.get('code', '').splitlines())}")
            
            # Show a snippet of the generated code
            code = udf_data.get('code', '')
            if code:
                code_lines = code.strip().splitlines()
                if len(code_lines) > 2:
                    print(f"   Code snippet: {code_lines[1]} ... (+ {len(code_lines)-3} more lines)")
                else:
                    print(f"   Code snippet: {code_lines[0] if code_lines else 'N/A'}")
            
            # Show dependencies if any
            if 'depends_on' in step:
                print(f"   Dependencies: {step['depends_on']}")
        
        # Create a complete StepFlow workflow
        complete_workflow = {
            'name': langflow_data.get('name', 'Translated Langflow Workflow'),
            'description': f"Converted from Langflow using UDF-based translation. Original: {langflow_data.get('description', '')}",
            'input_schema': {
                'type': 'object',
                'properties': {
                    'user_input': {
                        'type': 'string',
                        'description': 'User input message'
                    }
                },
                'required': ['user_input']
            },
            'steps': workflow_steps,
            'output': {
                # For now, output the last step's result
                'result': {
                    '$from': {'step': workflow_steps[-1]['id'] if workflow_steps else 'unknown'},
                    'path': 'blob_id'
                }
            }
        }
        
        # Save to a new YAML file
        output_file = Path(__file__).parent / "translated_workflow_simple.yaml"
        with open(output_file, 'w') as f:
            yaml.dump(complete_workflow, f, default_flow_style=False, indent=2, sort_keys=False)
        
        print(f"\n✅ Translation completed successfully!")
        print(f"Full workflow saved to: {output_file}")
        
        # Show the complete workflow structure
        print("\n=== Complete StepFlow Workflow Structure ===")
        print(f"Name: {complete_workflow['name']}")
        print(f"Steps: {len(complete_workflow['steps'])}")
        print(f"Has input schema: {bool(complete_workflow.get('input_schema'))}")
        print(f"Has output mapping: {bool(complete_workflow.get('output'))}")
        
        return 0
        
    except Exception as e:
        print(f"❌ Translation failed: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    print("🔄 Testing Simple Langflow to StepFlow UDF Translator")
    print("=" * 50)
    
    result = test_langflow_translation()
    
    print("\n" + "=" * 50)
    print("Test completed!")
    
    exit(result)