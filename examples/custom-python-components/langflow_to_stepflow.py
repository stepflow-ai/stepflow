#!/usr/bin/env python3
"""
Langflow to Stepflow Translation Layer

This script converts Langflow JSON workflow files to Stepflow YAML format.
It provides a reusable translation layer that maps Langflow components to
equivalent Stepflow workflow steps.
"""

import json
import yaml
import subprocess
import tempfile
import re
from typing import Dict, List, Any, Optional
from pathlib import Path
import argparse


class StepflowSchemaDiscovery:
    """Discovers component schemas from StepFlow runtime."""
    
    def __init__(self, stepflow_path: str = "cargo run --"):
        self.stepflow_path = stepflow_path
        self._schema_cache = {}
    
    def get_component_schemas(self) -> Dict[str, Dict[str, Any]]:
        """Get all component schemas from StepFlow."""
        if not self._schema_cache:
            try:
                # Determine the correct working directory for stepflow-rs
                script_dir = Path(__file__).parent
                stepflow_rs_dir = script_dir / "../../stepflow-rs"
                stepflow_rs_dir = stepflow_rs_dir.resolve()
                
                if not stepflow_rs_dir.exists():
                    print(f"Warning: stepflow-rs directory not found at {stepflow_rs_dir}")
                    return self._schema_cache
                
                # Run stepflow list-components with schemas
                cmd_parts = self.stepflow_path.split()
                cmd_parts.extend(["list-components", "--format=json", "--schemas=true"])
                
                print(f"DEBUG: Running command: {' '.join(cmd_parts)} in {stepflow_rs_dir}")
                
                result = subprocess.run(
                    cmd_parts,
                    capture_output=True,
                    text=True,
                    cwd=str(stepflow_rs_dir)
                )
                
                print(f"DEBUG: Command return code: {result.returncode}")
                print(f"DEBUG: Command stderr: {result.stderr}")
                print(f"DEBUG: Command stdout: {repr(result.stdout[:500])}")  # Show first 500 chars
                
                if result.returncode == 0:
                    # Remove ANSI color codes from output
                    ansi_escape = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
                    clean_output = ansi_escape.sub('', result.stdout)
                    
                    # Find the JSON part of the output
                    # Look for the opening brace that starts the JSON
                    json_start = clean_output.find('{\n  "components"')
                    if json_start == -1:
                        # Try alternative patterns
                        json_start = clean_output.find('{"components"')
                    
                    if json_start != -1:
                        json_text = clean_output[json_start:].strip()
                        
                        print(f"DEBUG: Extracted JSON text: {repr(json_text[:200])}")
                        
                        try:
                            data = json.loads(json_text)
                            components = data.get('components', [])
                            # Index by component name for easy lookup
                            for component in components:
                                component_name = component.get('component')
                                if component_name:
                                    # Add builtin:// prefix if not present
                                    if not component_name.startswith('builtin://'):
                                        component_name = f"builtin://{component_name}"
                                    self._schema_cache[component_name] = component
                                    
                            print(f"DEBUG: Successfully loaded {len(self._schema_cache)} component schemas")
                        except json.JSONDecodeError as e:
                            print(f"Warning: JSON parsing failed: {e}")
                            print(f"JSON text: {repr(json_text[:500])}")
                    else:
                        print(f"Warning: No JSON found in output")
                        print(f"Clean output: {repr(clean_output[:500])}")
                else:
                    print(f"Warning: Failed to get component schemas: {result.stderr}")
            except Exception as e:
                print(f"Warning: Schema discovery failed: {e}")
                import traceback
                traceback.print_exc()
                
        return self._schema_cache
    
    def get_output_schema(self, component_name: str) -> Optional[Dict[str, Any]]:
        """Get the output schema for a specific component."""
        schemas = self.get_component_schemas()
        component_info = schemas.get(component_name)
        if component_info and 'output_schema' in component_info:
            return component_info['output_schema']
        return None
    
    def get_primary_output_field(self, component_name: str) -> Optional[str]:
        """Get the primary output field name for a component."""
        schema = self.get_output_schema(component_name)
        if schema and 'properties' in schema:
            properties = schema['properties']
            if len(properties) == 1:
                # Single field - return it
                return list(properties.keys())[0]
            elif 'response' in properties:
                # Common pattern - prefer 'response' field
                return 'response'
            elif 'content' in properties:
                # Alternative common pattern
                return 'content'
            else:
                # Default to first field
                return list(properties.keys())[0]
        return None


class LangflowToStepflowTranslator:
    """Translates Langflow workflows to Stepflow format."""
    
    def __init__(self, stepflow_path: Optional[str] = "cargo run --"):
        # Component mapping from Langflow to Stepflow
        self.component_mappings = {
            'ChatInput': self._translate_chat_input,
            'Prompt': self._translate_prompt, 
            'LanguageModelComponent': self._translate_language_model,
            'ChatOutput': self._translate_chat_output
        }
        
        # Track translated steps for dependency resolution
        self.translated_steps = {}
        self.edges = []
        
        # Schema discovery (optional)
        self.schema_discovery = StepflowSchemaDiscovery(stepflow_path) if stepflow_path else None
    
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
        
        print(f"DEBUG: Node dependencies: {dependencies}")
        
        # Perform topological sort using Kahn's algorithm
        # Calculate in-degree for each node
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
            # Circular dependency detected
            remaining = [node_id for node_id in node_map if node_id not in [n['id'] for n in sorted_nodes]]
            # TODO: FRAZ - loop handling?
            print(f"WARNING: Circular dependency detected. Remaining nodes: {remaining}")
            # Add remaining nodes in original order
            for node in nodes:
                if node not in sorted_nodes:
                    sorted_nodes.append(node)
        
        print(f"DEBUG: Topologically sorted node order: {[n['id'] for n in sorted_nodes]}")
        return sorted_nodes

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
                step_info = self.component_mappings[node_type](node, dependencies)
                if step_info:
                    if step_info.get('type') == 'input_schema':
                        input_schema = step_info['schema']
                    elif step_info.get('type') == 'output_mapping':
                        output_mapping = step_info['mapping']
                    elif step_info.get('type') == 'multi_step':
                        # Handle multi-step results (like prompt blob storage)
                        for step in step_info['steps']:
                            steps.append(step)
                        # Store the final step for dependency resolution
                        final_step_id = step_info['final_step_id']
                        final_step = next(s for s in step_info['steps'] if s['id'] == final_step_id)
                        self.translated_steps[node['id']] = final_step
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
        template = node.get('data', {}).get('node', {}).get('template', {})
        input_value = template.get('input_value', {}).get('value', '')
        
        # ChatInput defines the workflow input schema
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
        """Store prompt as blob and create get_blob step to retrieve it."""
        node_id = node['id']
        template = node.get('data', {}).get('node', {}).get('template', {})
        prompt_template = template.get('template', {}).get('value', '')
        
        # Create put_blob step to store the prompt
        put_blob_step = {
            'id': f'put_prompt_{node_id.split("-")[1]}',
            'component': 'builtin://put_blob',
            'input': {
                'data': prompt_template
            }
        }
        
        # Create get_blob step to retrieve the prompt content
        get_blob_step = {
            'id': f'get_prompt_{node_id.split("-")[1]}',
            'component': 'builtin://get_blob',
            'input': {
                'blob_id': {'$from': {'step': put_blob_step['id']}, 'path': 'blob_id'}
            }
        }
        
        # Return both steps as a multi-step result
        return {
            'type': 'multi_step',
            'steps': [put_blob_step, get_blob_step],
            'final_step_id': get_blob_step['id']  # This is what other steps should reference
        }
    
    def _translate_language_model(self, node: Dict[str, Any], dependencies: Dict[str, List]) -> Optional[Dict[str, Any]]:
        """Translate LanguageModelComponent to OpenAI component."""
        node_id = node['id']
        template = node.get('data', {}).get('node', {}).get('template', {})
        
        # Extract model configuration
        provider = template.get('provider', {}).get('value', 'OpenAI')
        model_name = template.get('model_name', {}).get('value', 'gpt-4o-mini')
        temperature = template.get('temperature', {}).get('value', 0.1)
        api_key = template.get('api_key', {}).get('value', 'OPENAI_API_KEY')
        
        print(f"DEBUG: LanguageModel node_id={node_id}, provider={provider}, model={model_name}, temperature={temperature}, api_key={api_key}")
        
        # Build input references from dependencies
        input_args = {}
        system_prompt_content = None
        
        deps = dependencies.get(node_id, [])
        print(f"DEBUG: LanguageModel dependencies={deps}")
        
        for dep in deps:
            if dep['target_input'] == 'input_value':
                # User message input
                if 'messages' not in input_args:
                    input_args['messages'] = []
                input_args['messages'].append({
                    'role': 'user', 
                    'content': {'$from': {'workflow': 'input'}, 'path': 'message'}
                })
            elif dep['target_input'] == 'system_message':
                # Reference the system prompt from the source step output using dynamic field discovery
                source_id = dep['source_id']
                source_step = self.translated_steps.get(source_id)
                if source_step:
                    source_component = source_step.get('component')
                    source_step_id = source_step.get('id')
                    
                    # Dynamically discover the output field from the source component
                    output_field = None
                    if self.schema_discovery and source_component:
                        output_field = self.schema_discovery.get_primary_output_field(source_component)
                        print(f"DEBUG: Discovered field '{output_field}' for component '{source_component}'")
                    
                    if not output_field:
                        # No fallbacks - this is an error that needs to be fixed
                        raise ValueError(f"Could not discover output field for component '{source_component}'. Schema discovery failed and no fallbacks allowed.")
                    
                    print(f"DEBUG: System message from step {source_step_id}, component {source_component}, field {output_field}")
                    
                    # Create a StepFlow reference using dynamic field discovery
                    if 'messages' not in input_args:
                        input_args['messages'] = []
                    input_args['messages'].insert(0, {
                        'role': 'system',
                        'content': {'$from': {'step': source_step_id}, 'path': output_field}
                    })
                    # Set flag to prevent duplicate system message addition
                    system_prompt_content = "REFERENCED"  # Flag to prevent duplicate
                else:
                    print(f"DEBUG: Could not find source step {source_id} for system message")
        
        # Add system message if found (and not already referenced)
        if system_prompt_content and system_prompt_content != "REFERENCED":
            if 'messages' not in input_args:
                input_args['messages'] = []
            input_args['messages'].insert(0, {
                'role': 'system',
                'content': system_prompt_content
            })
        
        # Default to simple user message if no dependencies
        if not input_args:
            input_args['messages'] = [
                {'role': 'user', 'content': {'$from': {'workflow': 'input'}, 'path': 'message'}}
            ]
        
        # Build input args without API key (will use environment variable)
        input_data = {
            'model': model_name,
            'temperature': temperature,
            **input_args
        }
        
        # Only include API key if it's not the default placeholder
        if api_key and api_key != 'OPENAI_API_KEY':
            input_data['api_key'] = api_key
        
        result = {
            'id': f'llm_call_{node_id.split("-")[1]}',
            'component': 'builtin://openai',
            'input': input_data
        }
        
        print(f"DEBUG: LanguageModel returning: {result}")
        return result
    
    def _translate_chat_output(self, node: Dict[str, Any], dependencies: Dict[str, List]) -> Optional[Dict[str, Any]]:
        """Translate ChatOutput to output mapping."""
        node_id = node['id']
        
        # Find the source of the output
        deps = dependencies.get(node_id, [])
        if deps:
            source_dep = deps[0]  # Assuming single input
            source_step_id = f"llm_call_{source_dep['source_id'].split('-')[1]}"
            
            # Find the source step to get its component
            source_step = self.translated_steps.get(source_dep['source_id'])
            component_name = None
            if source_step:
                component_name = source_step.get('component')
            
            # Debug prints
            print(f"DEBUG: ChatOutput node_id={node_id}, source_dep={source_dep}")
            print(f"DEBUG: source_step_id={source_step_id}, component_name={component_name}")
            print(f"DEBUG: source_step={source_step}")
            
            # Dynamically discover the output field from the actual component
            output_field = None
            if self.schema_discovery and component_name:
                output_field = self.schema_discovery.get_primary_output_field(component_name)
                print(f"DEBUG: Discovered output_field='{output_field}' for component='{component_name}'")
            
            if not output_field:
                # No fallbacks - this is an error that needs to be fixed
                raise ValueError(f"Could not discover output field for component '{component_name}'. Schema discovery failed and no fallbacks allowed.")
            
            result = {
                'type': 'output_mapping',
                'mapping': {
                    'response': {'$from': {'step': source_step_id}, 'path': output_field}
                }
            }
            
            print(f"DEBUG: ChatOutput returning: {result}")
            return result
        
        return None


def main():
    """CLI interface for the translator."""
    parser = argparse.ArgumentParser(description='Convert Langflow JSON to Stepflow YAML')
    parser.add_argument('input_file', help='Input Langflow JSON file')
    parser.add_argument('output_file', help='Output Stepflow YAML file')
    parser.add_argument('--pretty', action='store_true', help='Pretty print YAML output')
    parser.add_argument('--stepflow-path', default='cargo run --', 
                      help='Path to stepflow binary (default: "cargo run --")')
    parser.add_argument('--no-schema-discovery', action='store_true',
                      help='Disable dynamic schema discovery (use hardcoded fallbacks)')
    
    args = parser.parse_args()
    
    # Load Langflow JSON
    with open(args.input_file, 'r') as f:
        langflow_data = json.load(f)
    
    # Translate to Stepflow
    stepflow_path = args.stepflow_path if not args.no_schema_discovery else None
    translator = LangflowToStepflowTranslator(stepflow_path)
    stepflow_data = translator.translate(langflow_data)
    
    # Save Stepflow YAML
    with open(args.output_file, 'w') as f:
        yaml.dump(stepflow_data, f, default_flow_style=False, indent=2, sort_keys=False)
    
    print(f"Successfully converted {args.input_file} to {args.output_file}")
    
    # Print schema discovery info if enabled
    if not args.no_schema_discovery:
        schemas = translator.schema_discovery.get_component_schemas()
        if schemas:
            print(f"Discovered {len(schemas)} component schemas from StepFlow runtime")
        else:
            print("Warning: No component schemas discovered - using fallback field names")


if __name__ == '__main__':
    main()