#!/usr/bin/env python3
"""
Enhanced Langflow to Stepflow Translation Layer - Native Types Version

This version leverages native Langflow types and components instead of built-ins,
providing better compatibility and reduced conversion overhead.
"""

import json
import yaml
from typing import Dict, List, Any, Optional
from pathlib import Path
import argparse


class NativeLangflowTranslator:
    """Translates Langflow workflows to Stepflow format using native Langflow components."""
    
    def __init__(self):
        # Component mapping from Langflow to native Stepflow components
        self.component_mappings = {
            'ChatInput': self._translate_chat_input,
            'Prompt': self._translate_prompt, 
            'LanguageModelComponent': self._translate_language_model,
            'ChatOutput': self._translate_chat_output
        }
        
        # Track translated steps for dependency resolution
        self.translated_steps = {}
        self.edges = []
    
    def _extract_component_inputs(self, node: Dict[str, Any]) -> Dict[str, Any]:
        """Generic function to extract input parameters from any Langflow component node."""
        template = node.get('data', {}).get('node', {}).get('template', {})
        inputs = {}
        
        # Common placeholders to skip
        placeholders = {'OPENAI_API_KEY', 'YOUR_API_KEY', 'API_KEY', ''}
        
        for field_name, field_data in template.items():
            if isinstance(field_data, dict):
                value = field_data.get('value')
                
                # Handle different value types
                if value is not None:
                    # Skip empty strings and common placeholders
                    # TODO: FRAZ - empty string is different than None? 
                    if isinstance(value, str) and value in placeholders:
                        continue
                    
                    # Convert numeric strings to appropriate types
                    if isinstance(value, str) and value.replace('.', '').replace('-', '').isdigit():
                        try:
                            # Try int first, then float
                            if '.' in value:
                                inputs[field_name] = float(value)
                            else:
                                inputs[field_name] = int(value)
                        except ValueError:
                            inputs[field_name] = value
                    else:
                        inputs[field_name] = value
        
        return inputs
    
    def _get_component_dependencies(self, node_id: str, dependencies: Dict[str, List]) -> List[Dict[str, Any]]:
        """Get all dependencies for a specific node."""
        return dependencies.get(node_id, [])
    
    def _translate_generic_component(self, node: Dict[str, Any], dependencies: Dict[str, List], 
                                   component_name: str) -> Dict[str, Any]:
        """Generic translator for any Langflow component using native types."""
        node_id = node['id']
        inputs = self._extract_component_inputs(node)
        deps = self._get_component_dependencies(node_id, dependencies)
        
        # Build input data from extracted inputs
        input_data = inputs.copy()
        
        # Handle dependencies by mapping them to input fields
        for dep in deps:
            target_input = dep['target_input']
            source_step = self.translated_steps.get(dep['source_id'])
            
            if source_step:
                # Create reference to source step output
                # For native Langflow components, we can use common field names
                if target_input == 'messages' or target_input == 'input_value':
                    # Handle message inputs
                    if 'messages' not in input_data:
                        input_data['messages'] = []
                    
                    # Determine message role based on source
                    role = 'user'  # Default
                    if 'prompt' in source_step.get('component', '').lower():
                        role = 'system'
                    
                    input_data['messages'].append({
                        'role': role,
                        'content': {'$from': {'step': source_step['id']}, 'path': 'formatted_prompt'}
                    })
                else:
                    # Direct field mapping
                    input_data[target_input] = {
                        '$from': {'step': source_step['id']},
                        'path': 'result'  # Default output path for native components
                    }
        
        return {
            'id': f'{component_name.lower()}_{node_id.split("-")[-1]}',
            'component': f'langflow://{component_name}',
            'input': input_data
        }
    
    def translate(self, langflow_json: Dict[str, Any]) -> Dict[str, Any]:
        """Main translation method."""
        
        # Extract basic workflow metadata
        workflow_name = langflow_json.get('name', 'Translated Workflow')
        description = langflow_json.get('description', 'Converted from Langflow')
        
        # Parse nodes and edges
        nodes = langflow_json.get('data', {}).get('nodes', [])
        self.edges = langflow_json.get('data', {}).get('edges', [])
        
        # Build step dependencies from edges
        dependencies = self._build_dependencies()
        
        # Sort nodes in dependency order (topological sort)
        sorted_nodes = self._topological_sort_nodes(nodes)
        
        # Translate nodes to steps in dependency order
        steps = []
        input_schema = None
        output_mapping = {}
        
        for node in sorted_nodes:
            node_type = node.get('data', {}).get('type')
            
            if node_type in self.component_mappings:
                # Use specific translator for known components
                step_info = self.component_mappings[node_type](node, dependencies)
            else:
                # Use generic translator for unknown components
                print(f"INFO: Using generic translator for component type: {node_type}")
                step_info = self._translate_generic_component(node, dependencies, node_type)
            
            if step_info:
                if step_info.get('type') == 'input_schema':
                    input_schema = step_info['schema']
                elif step_info.get('type') == 'output_mapping':
                    output_mapping = step_info['mapping']
                else:
                    steps.append(step_info)
                    self.translated_steps[node['id']] = step_info
        
        # Build the final workflow
        workflow = {
            'name': workflow_name,
            'description': description
        }
        
        if input_schema:
            workflow['input_schema'] = input_schema
            
        if steps:
            workflow['steps'] = steps
            
        if output_mapping:
            workflow['output'] = output_mapping
            
        return workflow
    
    def _topological_sort_nodes(self, nodes: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """Sort nodes in dependency order using topological sort."""
        # Create a mapping from node ID to node
        node_map = {node['id']: node for node in nodes}
        
        # Build dependency graph: node_id -> list of nodes it depends on
        dependencies = {}
        for node in nodes:
            dependencies[node['id']] = []
        
        # Add dependencies from edges
        for edge in self.edges:
            target_id = edge['target']
            source_id = edge['source']
            if target_id in dependencies and source_id in node_map:
                dependencies[target_id].append(source_id)
        
        # Perform topological sort using Kahn's algorithm
        in_degree = {node_id: 0 for node_id in node_map}
        for node_id, deps in dependencies.items():
            in_degree[node_id] = len(deps)
        
        # Start with nodes that have no dependencies
        queue = [node_id for node_id, degree in in_degree.items() if degree == 0]
        sorted_nodes = []
        
        while queue:
            current_id = queue.pop(0)
            sorted_nodes.append(node_map[current_id])
            
            # Remove this node from dependencies of other nodes
            for node_id, deps in dependencies.items():
                if current_id in deps:
                    deps.remove(current_id)
                    in_degree[node_id] -= 1
                    if in_degree[node_id] == 0:
                        queue.append(node_id)
        
        if len(sorted_nodes) != len(nodes):
            # Circular dependency detected - add remaining nodes
            for node in nodes:
                if node not in sorted_nodes:
                    sorted_nodes.append(node)
        
        return sorted_nodes
    
    def _build_dependencies(self) -> Dict[str, List[Dict[str, Any]]]:
        """Build dependency graph from edges."""
        dependencies = {}
        
        for edge in self.edges:
            target_id = edge['target']
            source_id = edge['source']
            
            # Extract connection information
            source_handle = edge.get('data', {}).get('sourceHandle', {})
            target_handle = edge.get('data', {}).get('targetHandle', {})
            
            if target_id not in dependencies:
                dependencies[target_id] = []
                
            dependencies[target_id].append({
                'source_id': source_id,
                'source_output': source_handle.get('name', 'output'),
                'target_input': target_handle.get('fieldName', 'input')
            })
        
        return dependencies
    
    def _translate_chat_input(self, node: Dict[str, Any], dependencies: Dict[str, List]) -> Optional[Dict[str, Any]]:
        """Translate ChatInput to input schema."""
        inputs = self._extract_component_inputs(node)
        input_value = inputs.get('input_value', '')
        
        return {
            'type': 'input_schema',
            'schema': {
                'type': 'object',
                'properties': {
                    'message': {
                        'type': 'string',
                        'description': 'User input message',
                        'default': input_value
                    }
                },
                'required': ['message']
            }
        }
    
    def _translate_prompt(self, node: Dict[str, Any], dependencies: Dict[str, List]) -> Optional[Dict[str, Any]]:
        """Translate Prompt to native Langflow prompt_formatter component."""
        node_id = node['id']
        inputs = self._extract_component_inputs(node)
        prompt_template = inputs.get('template', '')
        
        # Extract variables from template (simple regex pattern)
        import re
        variables = {}
        variable_names = re.findall(r'\{(\w+)\}', prompt_template)
        for var_name in variable_names:
            if var_name != 'message':  # Reserve 'message' for user input
                variables[var_name] = f"{{default_{var_name}}}"
        
        # Use native langflow prompt_formatter
        return {
            'id': f'format_prompt_{node_id.split("-")[1]}',
            'component': 'langflow://prompt_formatter',
            'input': {
                'template': prompt_template,
                'variables': variables
            }
        }
    
    def _translate_language_model(self, node: Dict[str, Any], dependencies: Dict[str, List]) -> Optional[Dict[str, Any]]:
        """Translate LanguageModelComponent to native Langflow OpenAI component."""
        node_id = node['id']
        inputs = self._extract_component_inputs(node)
        
        # Extract model configuration using generic extraction
        model_name = inputs.get('model_name', 'gpt-4o-mini')
        temperature = inputs.get('temperature', 0.1)
        provider = inputs.get('provider', 'OpenAI')
        api_key = inputs.get('api_key')  # Will be None if not set or is placeholder
        
        # Build native Langflow input format
        input_data = {
            'model': model_name,
            'temperature': temperature
        }
        
        # Add API key if provided and not a placeholder
        if api_key:
            input_data['api_key'] = api_key
        
        # Handle message inputs from dependencies
        deps = self._get_component_dependencies(node_id, dependencies)
        messages = []
        
        for dep in deps:
            if dep['target_input'] == 'system_message':
                # System message from prompt component
                source_step = self.translated_steps.get(dep['source_id'])
                if source_step:
                    # Create system message using native Message object
                    messages.append({
                        'role': 'system',
                        'content': {'$from': {'step': source_step['id']}, 'path': 'formatted_prompt'}
                    })
            elif dep['target_input'] == 'input_value':
                # User message from input
                messages.append({
                    'role': 'user',
                    'content': {'$from': {'workflow': 'input'}, 'path': 'message'}
                })
        
        # Default to simple user message if no dependencies
        if not messages:
            messages = [
                {'role': 'user', 'content': {'$from': {'workflow': 'input'}, 'path': 'message'}}
            ]
        
        input_data['messages'] = messages
        
        return {
            'id': f'llm_call_{node_id.split("-")[1]}',
            'component': 'langflow://openai_chat',  # Use native Langflow component
            'input': input_data
        }
    
    def _translate_chat_output(self, node: Dict[str, Any], dependencies: Dict[str, List]) -> Optional[Dict[str, Any]]:
        """Translate ChatOutput to output mapping using native types."""
        node_id = node['id']
        
        # Find the source of the output
        deps = dependencies.get(node_id, [])
        if deps:
            source_dep = deps[0]  # Assuming single input
            source_step_id = f"llm_call_{source_dep['source_id'].split('-')[1]}"
            
            # Native Langflow components return structured objects
            # OpenAI chat component returns a Message object with 'text' field
            return {
                'type': 'output_mapping',
                'mapping': {
                    'response': {'$from': {'step': source_step_id}, 'path': 'text'}
                }
            }
        
        return None


def main():
    """CLI interface for the native translator."""
    parser = argparse.ArgumentParser(description='Convert Langflow JSON to Stepflow YAML using native types')
    parser.add_argument('input_file', help='Input Langflow JSON file')
    parser.add_argument('output_file', help='Output Stepflow YAML file')
    parser.add_argument('--pretty', action='store_true', help='Pretty print YAML output')
    
    args = parser.parse_args()
    
    # Load Langflow JSON
    with open(args.input_file, 'r') as f:
        langflow_data = json.load(f)
    
    # Translate to Stepflow using native types
    translator = NativeLangflowTranslator()
    stepflow_data = translator.translate(langflow_data)
    
    # Save Stepflow YAML
    with open(args.output_file, 'w') as f:
        yaml.dump(stepflow_data, f, default_flow_style=False, indent=2, sort_keys=False)
    
    print(f"Successfully converted {args.input_file} to {args.output_file}")
    print("Translation uses native Langflow components for optimal performance.")


if __name__ == '__main__':
    main()