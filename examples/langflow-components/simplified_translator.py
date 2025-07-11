#!/usr/bin/env python3
"""
Simplified Langflow to Stepflow translator for native components.

This version does minimal translation since native Langflow components
can handle the original Langflow data directly.
"""

import json
import yaml
from typing import Dict, List, Any, Optional

class SimplifiedLangflowTranslator:
    """Minimal translator that passes native Langflow data through."""
    
    def __init__(self):
        # Map Langflow component types to native StepFlow component names
        self.component_map = {
            'ChatInput': 'workflow_input',  # Special case
            'ChatOutput': 'workflow_output',  # Special case
            'Prompt': 'langflow://prompt_formatter',
            'LanguageModelComponent': 'langflow://openai_chat',
            'TextSplitter': 'langflow://text_splitter',
            # Add more as needed - this is just a name mapping
        }
        
        self.translated_steps = {}
        self.edges = []
    
    def translate(self, langflow_json: Dict[str, Any]) -> Dict[str, Any]:
        """Translate with minimal processing."""
        
        nodes = langflow_json.get('data', {}).get('nodes', [])
        self.edges = langflow_json.get('data', {}).get('edges', [])
        
        # Build dependencies
        dependencies = self._build_dependencies()
        
        # Sort nodes
        sorted_nodes = self._topological_sort_nodes(nodes)
        
        steps = []
        input_schema = None
        output_mapping = {}
        
        for node in sorted_nodes:
            node_type = node.get('data', {}).get('type')
            
            # Skip nodes without a type (like note nodes)
            if not node_type:
                continue
                
            if node_type == 'ChatInput':
                input_schema = self._create_input_schema(node)
            elif node_type == 'ChatOutput':
                output_mapping = self._create_output_mapping(node, dependencies)
            else:
                # For all other components, minimal translation
                step = self._translate_to_native_component(node, dependencies)
                if step:
                    steps.append(step)
                    self.translated_steps[node['id']] = step
        
        return {
            'name': langflow_json.get('name', 'Translated Workflow'),
            'description': langflow_json.get('description', 'Converted from Langflow'),
            'input_schema': input_schema,
            'steps': steps,
            'output': output_mapping
        }
    
    def _translate_to_native_component(self, node: Dict[str, Any], dependencies: Dict[str, List]) -> Optional[Dict[str, Any]]:
        """Translate any component to native format with minimal processing."""
        node_id = node['id']
        node_type = node.get('data', {}).get('type')
        
        # Get the StepFlow component name
        component_name = self.component_map.get(node_type, f'langflow://{node_type.lower()}')
        
        # Pass the raw Langflow template data directly to the native component
        template = node.get('data', {}).get('node', {}).get('template', {})
        
        # Extract just the values (the native component can handle the rest)
        raw_inputs = {}
        skip_fields = {'code', 'background_color', 'chat_icon', 'text_color', 'sender', 'sender_name', 'session_id', 'files', 'should_store_message', 'clean_data', 'data_template', 'tool_placeholder'}
        skip_values = {'OPENAI_API_KEY', 'YOUR_API_KEY', 'API_KEY', ''}
        
        for field_name, field_data in template.items():
            if field_name in skip_fields:
                continue
                
            if isinstance(field_data, dict) and 'value' in field_data:
                value = field_data['value']
                if value is not None and value not in skip_values:
                    raw_inputs[field_name] = value
        
        # Handle dependencies
        deps = dependencies.get(node_id, [])
        for dep in deps:
            source_step = self.translated_steps.get(dep['source_id'])
            if source_step:
                # Simple reference - let the native component figure out the details
                raw_inputs[dep['target_input']] = {
                    '$from': {'step': source_step['id']},
                    'path': 'result'  # Generic output path
                }
        
        return {
            'id': f'{node_type.lower()}_{node_id.split("-")[-1]}',
            'component': component_name,
            'input': raw_inputs
        }
    
    def _create_input_schema(self, node: Dict[str, Any]) -> Dict[str, Any]:
        """Create workflow input schema from ChatInput."""
        template = node.get('data', {}).get('node', {}).get('template', {})
        default_value = template.get('input_value', {}).get('value', '')
        
        return {
            'type': 'object',
            'properties': {
                'message': {
                    'type': 'string',
                    'description': 'User input message',
                    'default': default_value
                }
            },
            'required': ['message']
        }
    
    def _create_output_mapping(self, node: Dict[str, Any], dependencies: Dict[str, List]) -> Dict[str, Any]:
        """Create workflow output mapping from ChatOutput."""
        deps = dependencies.get(node['id'], [])
        if deps:
            source_dep = deps[0]
            source_step = self.translated_steps.get(source_dep['source_id'])
            if source_step:
                return {
                    'response': {
                        '$from': {'step': source_step['id']},
                        'path': 'text'  # Native Langflow Message objects have 'text' field
                    }
                }
        return {'response': 'No output source found'}
    
    def _build_dependencies(self) -> Dict[str, List[Dict[str, Any]]]:
        """Build dependency graph from edges."""
        dependencies = {}
        for edge in self.edges:
            target_id = edge['target']
            source_id = edge['source']
            
            if target_id not in dependencies:
                dependencies[target_id] = []
                
            dependencies[target_id].append({
                'source_id': source_id,
                'target_input': edge.get('data', {}).get('targetHandle', {}).get('fieldName', 'input')
            })
        
        return dependencies
    
    def _topological_sort_nodes(self, nodes: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """Simple topological sort."""
        # Simplified version - in practice you'd want the full implementation
        return sorted(nodes, key=lambda n: len([e for e in self.edges if e['target'] == n['id']]))


def main():
    """Demo the simplified translator."""
    import argparse
    
    parser = argparse.ArgumentParser(description='Simplified Langflow to StepFlow translator')
    parser.add_argument('input_file', help='Input Langflow JSON file')
    parser.add_argument('output_file', help='Output StepFlow YAML file')
    
    args = parser.parse_args()
    
    with open(args.input_file, 'r') as f:
        langflow_data = json.load(f)
    
    translator = SimplifiedLangflowTranslator()
    stepflow_data = translator.translate(langflow_data)
    
    with open(args.output_file, 'w') as f:
        yaml.dump(stepflow_data, f, default_flow_style=False, indent=2)
    
    print(f"Simplified translation: {args.input_file} → {args.output_file}")
    print("Native components will handle the detailed schema interpretation.")


if __name__ == '__main__':
    main()