#!/usr/bin/env python3
"""
CLI for UDF-based Langflow to StepFlow translator.
Updated to use new parsing functions and generate workflows compatible 
with langflow_component_server_udf_only.py
"""

import argparse
import json
import yaml
from pathlib import Path
from typing import Dict, Any, List, Optional


# Add the parsing functions from the new translator
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
    """Generate a simplified input schema - let Langflow handle the complex validation."""
    
    # For now, create a very simple schema that allows any inputs
    # The UDF blob will contain the full Langflow template for proper validation
    properties = {}
    required = []
    
    # Look for common input fields and create a basic schema
    for field_name, field_config in template.items():
        if field_name == '_type' or not isinstance(field_config, dict):
            continue
            
        # Include common execution fields with basic types
        if field_name in {'input_value', 'template', 'model_name', 'temperature', 'api_key', 'system_message'}:
            field_type = field_config.get('type', 'str')
            json_type = _map_langflow_type_to_json_schema(field_type)
            field_info = field_config.get('info', '')
            
            properties[field_name] = {
                'type': json_type,
                'description': field_info
            }
            
            if field_config.get('required', False):
                required.append(field_name)
    
    return {
        'type': 'object',
        'properties': properties,
        'required': required
    }

def _prepare_native_template(template: Dict[str, Any]) -> Dict[str, Any]:
    """Prepare the native Langflow template with all field data preserved."""
    
    # Instead of filtering, we'll keep the full template but ensure proper structure
    # This allows Langflow components to use their native validation and defaults
    prepared_template = {}
    
    for field_name, field_config in template.items():
        if field_name.startswith('_'):
            continue
            
        if isinstance(field_config, dict):
            # Keep the full field configuration for native Langflow processing
            prepared_template[field_name] = field_config
    
    return prepared_template


def _prepare_native_outputs(outputs: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    """Prepare native Langflow outputs with essential execution data."""
    prepared_outputs = []
    
    for output in outputs:
        # Keep the essential fields but allow the full output structure
        prepared_output = {
            'method': output.get('method'),
            'name': output.get('name'),
            'types': output.get('types', [])
        }
        
        # Only add outputs that have method names (required for execution)
        if prepared_output['method']:
            prepared_outputs.append(prepared_output)
    
    return prepared_outputs


def _extract_class_name_from_code(code: str) -> str:
    """Extract the class name from Langflow component code using AST."""
    if not code:
        raise ValueError("No code provided for class name extraction")
    
    import ast
   
    # Parse the code into an AST
    tree = ast.parse(code)
    
    # Find all class definitions
    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            # Return the first class name found
            # (Langflow components typically have one main class)
            return node.name
    
    raise ValueError("No class definition found in the provided code")


def _extract_component_info(node_data: Dict[str, Any], node_info: Dict[str, Any]) -> Dict[str, Any]:
    """Extract component information from a Langflow node using native types."""
    template = node_info.get('template', {})
    
    
    # Extract simplified input schema for workflow-level validation
    input_schema = _generate_input_schema_from_template(template)
    
    # Prepare native template with full Langflow data
    native_template = _prepare_native_template(template)
    
    # Prepare native outputs with essential execution data
    native_outputs = _prepare_native_outputs(node_info.get('outputs', []))
    
    # Extract the component type and configuration
    component_type = node_data.get('type', 'unknown')
    
    # Get the original component code if available
    original_code = template.get('code', {}).get('value', '') if 'code' in template else ''
    
    # Extract the actual class name from the code (if code exists)
    if original_code:
        try:
            actual_class_name = _extract_class_name_from_code(original_code)
            if actual_class_name:
                component_type = actual_class_name  # Use the actual class name instead of the Langflow type
        except ValueError:
            # No class found in code, use the original component_type
            pass
    
    # Get the selected output method - this helps determine which method to execute
    selected_output = node_data.get('selected_output')
    
    # If no selected_output is specified, use the first output as default
    if not selected_output and native_outputs:
        selected_output = native_outputs[0].get('name')
        print(f"No selected_output for {component_type}, using first output: {selected_output}")
    
    return {
        'type': component_type,
        'template': native_template,  # Use native template with full data
        'input_schema': input_schema,  # Simplified schema for workflow validation
        'outputs': native_outputs,  # Use native outputs
        'original_code': original_code,
        'selected_output': selected_output
    }


def _returns_non_serializable_object(component_type: str, outputs: List[Dict[str, Any]], selected_output: Optional[str] = None) -> bool:
    """Determine if a component's selected output returns a non-serializable object that needs to be kept in memory."""
    
    # Known components that return complex, non-serializable objects
    non_serializable_components = {
        'OpenAIEmbeddings',
        'HuggingFaceEmbeddings', 
        'SentenceTransformerEmbeddings',
        'CohereEmbeddings',
        'BedrockEmbeddings'
    }
    
    # Check if the component type is known to return non-serializable objects
    if component_type in non_serializable_components:
        return True
    
    # If selected_output is specified, only check that specific output
    outputs_to_check = outputs
    if selected_output:
        outputs_to_check = [o for o in outputs if o.get('name') == selected_output]
        if not outputs_to_check:
            # If selected output not found, fall back to all outputs
            outputs_to_check = outputs
    
    # Check output types - if they're not Message, Data, DataFrame, or basic types, they're likely non-serializable
    serializable_types = ['Message', 'Data', 'DataFrame', 'str', 'int', 'float', 'bool', 'list', 'dict']
    for output in outputs_to_check:
        output_types = output.get('types', [])
        for output_type in output_types:
            if output_type not in serializable_types:
                return True
    
    return False


def _can_be_fused(source_component: Dict[str, Any], target_component: Dict[str, Any]) -> bool:
    """Determine if two components can be fused together."""
    
    source_type = source_component.get('type', '')
    target_type = target_component.get('type', '')
    
    # Don't fuse I/O components (they handle workflow boundaries)
    if source_type in ['ChatInput', 'ChatOutput'] or target_type in ['ChatInput', 'ChatOutput']:
        return False
    
    # Only fuse if the source's selected output returns non-serializable objects
    source_outputs = source_component.get('outputs', [])
    source_selected = source_component.get('selected_output')
    if not _returns_non_serializable_object(source_type, source_outputs, source_selected):
        return False
    
    # Also check if the target component's selected output returns non-serializable objects
    # If the target returns serializable objects, we should stop the fusion chain here
    # This prevents unnecessary fusion of components that don't need it
    target_outputs = target_component.get('outputs', [])
    target_selected = target_component.get('selected_output')
    if not _returns_non_serializable_object(target_type, target_outputs, target_selected):
        # The target can consume the non-serializable object and return something serializable
        # We should include the target in the fusion but stop there
        return True
    
    # Both source and target return non-serializable objects, continue fusion
    return True


def _find_fusion_chains(components: Dict[str, Dict[str, Any]], edges: List[Dict[str, Any]]) -> List[List[str]]:
    """Find chains of components that should be fused together."""
    
    # Build adjacency list from edges
    adjacency = {}
    for edge in edges:
        source = edge.get('source')
        target = edge.get('target')
        if source and target:
            if source not in adjacency:
                adjacency[source] = []
            adjacency[source].append(target)
    
    # Find fusion chains
    fusion_chains = []
    visited = set()
    
    for component_id, component_info in components.items():
        if component_id in visited:
            continue
        
        # Start a potential chain from this component
        current_chain = [component_id]
        current_id = component_id
        
        # Follow the chain as long as components can be fused
        while current_id in adjacency:
            next_components = adjacency[current_id]
            
            # For simplicity, only handle single outgoing edges for fusion
            if len(next_components) != 1:
                break
            
            next_id = next_components[0]
            next_component = components.get(next_id)
            
            if not next_component:
                break
            
            # Check if current and next can be fused
            current_component = components[current_id]
            if _can_be_fused(current_component, next_component):
                # Check if the next component's selected output returns serializable objects
                # If so, include it in the fusion but stop the chain there
                next_outputs = next_component.get('outputs', [])
                next_type = next_component.get('type', '')
                next_selected = next_component.get('selected_output')
                next_returns_non_serializable = _returns_non_serializable_object(next_type, next_outputs, next_selected)
                
                current_chain.append(next_id)
                
                if not next_returns_non_serializable:
                    # Next component's selected output returns serializable objects, stop fusion chain here
                    break
                
                current_id = next_id
            else:
                break
        
        # Only consider it a fusion chain if it has multiple components
        if len(current_chain) > 1:
            fusion_chains.append(current_chain)
            visited.update(current_chain)
        else:
            visited.add(component_id)
    
    return fusion_chains


def _generate_fused_udf_creation_step(chain_ids: List[str], components: Dict[str, Dict[str, Any]], dependencies: List[str], edge_mappings: Dict[str, List[Dict[str, str]]]) -> Dict[str, Any]:
    """Generate a UDF creation step for a fused component chain."""
    
    # Collect all components in the chain
    chain_components = []
    for component_id in chain_ids:
        component_info = components[component_id]
        chain_components.append({
            'code': component_info['original_code'],
            'component_type': component_info['type'],
            'template': component_info['template'],
            'outputs': component_info['outputs'],
            'selected_output': component_info['selected_output']
        })
    
    # Extract edge mappings within the fusion chain
    internal_edge_mappings = {}
    for component_id in chain_ids:
        if component_id in edge_mappings:
            # Only include edges where the source is also in the fusion chain
            internal_edges = []
            for edge in edge_mappings[component_id]:
                if edge['source'] in chain_ids:
                    internal_edges.append(edge)
            if internal_edges:
                internal_edge_mappings[component_id] = internal_edges
    
    # Create the fused UDF data
    fused_udf_data = {
        'components': chain_components,
        'execution_chain': [components[cid]['type'] for cid in chain_ids],
        'input_schema': components[chain_ids[0]]['input_schema'],  # Use first component's input schema
        'fusion_chain_ids': chain_ids,  # Keep track of original component IDs
        'internal_edge_mappings': internal_edge_mappings  # Edge mappings within the fusion chain
    }
    
    # Create a descriptive name using all component types in the fusion
    component_types = [components[cid]['type'] for cid in chain_ids]
    # Use the full component type names, just join them with underscores
    fused_name = '_'.join(component_types)
    
    step = {
        'id': f'create_fused_{fused_name}_udf',
        'component': 'builtin://put_blob',
        'input': {
            'data': fused_udf_data
        }
    }
    
    # Add dependencies if any
    if dependencies:
        step['depends_on'] = [f'create_{dep}_udf' for dep in dependencies if dep not in chain_ids]
    
    return step


def _generate_udf_creation_step(node_id: str, component_info: Dict[str, Any], dependencies: List[str]) -> Dict[str, Any]:
    """Generate a StepFlow step that creates a UDF blob for this component."""
    
    # Create the UDF blob data with only execution-critical information
    udf_data = {
        'input_schema': component_info['input_schema'],
        'code': component_info['original_code'],
        'component_type': component_info['type'],
        'outputs': component_info['outputs'],  # Include outputs metadata for execution method detection
        'template': component_info['template'],  # Include template for initialization
        'selected_output': component_info['selected_output']  # Include selected output for method detection
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
    Parse a Langflow JSON workflow and convert it to StepFlow workflow steps with component fusion.
    
    Args:
        langflow_json: The Langflow workflow JSON structure
        
    Returns:
        List of StepFlow workflow steps that create UDF blobs for components (some may be fused)
    """
    data = langflow_json.get('data', {})
    nodes = data.get('nodes', [])
    edges = data.get('edges', [])
    
    # Build dependency graph from edges
    dependencies = _build_dependency_graph(edges)
    
    # Extract component information for all nodes
    components = {}
    for node in nodes:
        node_data = node.get('data', {})
        node_id = node_data.get('id')
        node_type = node_data.get('type')
        
        # Skip note nodes and other non-component nodes
        if node_type in ['note', 'noteNode'] or not node_id:
            continue
            
        node_info = node_data.get('node', {})
        component_info = _extract_component_info(node_data, node_info)
        components[node_id] = component_info
    
    # Find fusion chains - components that should be combined into single UDFs
    fusion_chains = _find_fusion_chains(components, edges)
    
    # Generate workflow steps
    workflow_steps = []
    fused_components = set()
    
    # Parse edges to create field-level mappings (needed for fusion)
    edge_mappings = {}
    for edge in edges:
        source_id = edge.get('source')
        target_id = edge.get('target')
        
        # Extract field information from edge data
        edge_data = edge.get('data', {})
        target_handle = edge_data.get('targetHandle', {})
        
        if isinstance(target_handle, dict):
            target_field = target_handle.get('fieldName')
        elif isinstance(target_handle, str):
            # Parse JSON-like string format
            import json
            try:
                target_handle_data = target_handle.replace('œ', '"')
                target_handle_parsed = json.loads(target_handle_data)
                target_field = target_handle_parsed.get('fieldName')
            except:
                target_field = 'input_value'  # fallback
        else:
            target_field = 'input_value'  # fallback
        
        if source_id and target_id and target_field:
            if target_id not in edge_mappings:
                edge_mappings[target_id] = []
            
            edge_mappings[target_id].append({
                'source': source_id,
                'target_field': target_field
            })

    # First, create fused UDF steps for chains
    for chain in fusion_chains:
        # Collect dependencies for the entire chain (only external dependencies)
        chain_dependencies = []
        for component_id in chain:
            comp_deps = dependencies.get(component_id, [])
            for dep in comp_deps:
                if dep not in chain:  # Only external dependencies
                    chain_dependencies.append(dep)
        
        # Generate fused UDF step
        fused_step = _generate_fused_udf_creation_step(chain, components, chain_dependencies, edge_mappings)
        workflow_steps.append(fused_step)
        
        # Mark these components as already handled
        fused_components.update(chain)
    
    # Then, create individual UDF steps for remaining components
    for component_id, component_info in components.items():
        if component_id not in fused_components:
            # Generate individual UDF creation step
            comp_dependencies = dependencies.get(component_id, [])
            udf_step = _generate_udf_creation_step(component_id, component_info, comp_dependencies)
            workflow_steps.append(udf_step)
    
    return workflow_steps


class LangflowUDFTranslator:
    """Updated translator that generates workflows compatible with langflow_component_server_udf_only.py"""
    
    def __init__(self):
        self.translated_steps = {}
        self.dependencies = {}
    
    def translate(self, langflow_json: Dict[str, Any]) -> Dict[str, Any]:
        """Translate Langflow JSON to StepFlow YAML that works with langflow_component_server_udf_only.py"""
        
        # Parse into UDF creation steps
        udf_steps = parse_langflow_to_stepflow_workflow(langflow_json)
        
        # Find input and output nodes
        nodes = langflow_json.get('data', {}).get('nodes', [])
        edges = langflow_json.get('data', {}).get('edges', [])
        
        input_node = None
        output_node = None
        entry_nodes = []
        
        # Build a set of nodes that have incoming edges
        nodes_with_incoming = set()
        for edge in edges:
            nodes_with_incoming.add(edge.get('target'))
        
        # Find nodes with specific types or no incoming edges
        for node in nodes:
            node_id = node.get('id')
            node_type = node.get('data', {}).get('type')
            
            if node_type == 'ChatInput':
                input_node = node
            elif node_type == 'ChatOutput':
                output_node = node
            elif node_id not in nodes_with_incoming:
                # This is an entry node (no incoming edges)
                entry_nodes.append(node)
        
        # Generate input schema based on input node or entry nodes
        if input_node:
            input_schema = self._create_input_schema(input_node)
        elif entry_nodes:
            input_schema = self._create_input_schema_from_entry_nodes(entry_nodes)
        else:
            # Fallback to generic schema
            input_schema = {
                'type': 'object',
                'properties': {
                    'user_input': {'type': 'string', 'description': 'User input message'}
                },
                'required': ['user_input']
            }
        
        # Generate execution steps that use the UDF blobs
        execution_steps = self._create_execution_steps(langflow_json, udf_steps)
        
        # Combine UDF creation and execution steps, then sort to ensure proper dependency order
        all_steps = udf_steps + execution_steps
        
        # Sort execution steps by dependency order to avoid forward references
        all_steps = self._sort_steps_by_dependencies(all_steps)
        
        # Generate output mapping
        output_mapping = self._create_output_mapping(output_node, execution_steps) if output_node else {
            'result': {
                '$from': {'step': execution_steps[-1]['id'] if execution_steps else 'unknown'},
                'path': 'result'
            }
        }
        
        return {
            'name': langflow_json.get('name', 'Translated Langflow Workflow'),
            'description': f"Converted from Langflow using UDF execution. Original: {langflow_json.get('description', '')}",
            'input_schema': input_schema,
            'steps': all_steps,
            'output': output_mapping
        }
    
    def _create_input_schema(self, input_node: Dict[str, Any]) -> Dict[str, Any]:
        """Create workflow input schema from ChatInput node."""
        if not input_node:
            return {
                'type': 'object',
                'properties': {
                    'message': {'type': 'string', 'description': 'User input message'}
                },
                'required': ['message']
            }
        
        template = input_node.get('data', {}).get('node', {}).get('template', {})
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
    
    def _create_input_schema_from_entry_nodes(self, entry_nodes: List[Dict[str, Any]]) -> Dict[str, Any]:
        """Create workflow input schema from entry nodes (nodes with no incoming edges)."""
        properties = {}
        required = []
        
        for node in entry_nodes:
            node_id = node.get('id')
            node_type = node.get('data', {}).get('type', 'Unknown')
            template = node.get('data', {}).get('node', {}).get('template', {})
            
            # Find fields that likely need external input
            for field_name, field_config in template.items():
                if not isinstance(field_config, dict):
                    continue
                
                # Skip internal/advanced fields
                if field_name in ['code', '_type'] or field_config.get('advanced', False):
                    continue
                
                # Look for fields that are meant for user input
                # These typically have empty values and are not hidden
                field_value = field_config.get('value')
                is_list = field_config.get('list', False)
                is_required = field_config.get('required', False)
                is_shown = field_config.get('show', True)
                
                # Create a workflow input parameter for fields that need external data
                # Include fields that are:
                # - Required
                # - Empty lists
                # - Non-advanced and user-facing (likely to need input)
                # - Have tool_mode=True (explicitly marked for external input)
                tool_mode = field_config.get('tool_mode', False)
                
                if is_shown and (is_required or tool_mode or 
                                (is_list and isinstance(field_value, list)) or
                                (field_name in ['urls', 'url', 'input', 'query', 'text', 'prompt'])):
                    # Generate a unique parameter name for this node's field
                    param_name = f"{node_type}_{field_name}".replace('-', '_')
                    
                    # Determine the JSON schema type
                    field_type = field_config.get('type', 'str')
                    json_type = _map_langflow_type_to_json_schema(field_type)
                    
                    # Build the property definition
                    property_def = {
                        'type': json_type,
                        'description': field_config.get('info', f'Input for {node_type} {field_name}')
                    }
                    
                    if is_list:
                        property_def = {
                            'type': 'array',
                            'items': {'type': json_type},
                            'description': field_config.get('info', f'List input for {node_type} {field_name}')
                        }
                    
                    properties[param_name] = property_def
                    
                    if is_required:
                        required.append(param_name)
        
        # If no properties were found, create a generic input
        if not properties:
            return {
                'type': 'object',
                'properties': {
                    'input_data': {'type': 'string', 'description': 'Input data for the workflow'}
                },
                'required': ['input_data']
            }
        
        return {
            'type': 'object',
            'properties': properties,
            'required': required
        }
    
    def _create_execution_steps(self, langflow_json: Dict[str, Any], udf_steps: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """Create execution steps that use the UDF blobs with proper edge mappings."""
        execution_steps = []
        
        # Parse edges to create field-level mappings
        edges = langflow_json.get('data', {}).get('edges', [])
        edge_mappings = self._parse_edge_mappings(edges)
        
        # Find entry nodes (nodes with no incoming edges)
        nodes_with_incoming = set()
        for edge in edges:
            nodes_with_incoming.add(edge.get('target'))
        
        # Create a mapping of node IDs to their types
        node_id_to_type = {}
        for node in langflow_json.get('data', {}).get('nodes', []):
            node_id = node.get('id')
            node_type = node.get('data', {}).get('type')
            if node_id and node_type:
                node_id_to_type[node_id] = node_type
        
        # Create execution steps for each UDF
        for udf_step in udf_steps:
            udf_data = udf_step['input']['data']
            
            # Check if this is a fused UDF or single component UDF
            if 'components' in udf_data and 'execution_chain' in udf_data:
                # This is a fused UDF - handle it specially
                fusion_chain_ids = udf_data.get('fusion_chain_ids', [])
                primary_id = fusion_chain_ids[0] if fusion_chain_ids else 'unknown'
                
                # Extract the fused name from the UDF step ID to maintain consistency
                udf_step_id = udf_step['id']
                if udf_step_id.startswith('create_fused_') and udf_step_id.endswith('_udf'):
                    fused_name = udf_step_id[13:-4]  # Remove 'create_fused_' and '_udf'
                    execution_id = f'execute_fused_{fused_name}'
                else:
                    # Fallback
                    execution_id = f'execute_fused_{primary_id}'
                
                # Create execution step for the fused chain
                execution_step = {
                    'id': execution_id,
                    'component': 'langflow://udf_executor',
                    'input': {
                        'blob_id': {
                            '$from': {'step': udf_step['id']},
                            'path': 'blob_id'
                        }
                    }
                }
                
                # Add input mappings for ALL components in the fusion chain that have external inputs
                input_mappings = {}
                
                # Check each component in the fusion chain for external inputs
                for component_id in fusion_chain_ids:
                    component_edges = edge_mappings.get(component_id, [])
                    
                    for edge in component_edges:
                        source_node = edge['source']
                        target_field = edge['target_field']
                        
                        # Only include external inputs (sources not in the fusion chain)
                        if source_node not in fusion_chain_ids:
                            # Get the correct execution ID (handles fused components)
                            source_execution_id = self._get_fused_execution_id(source_node, udf_steps)
                            
                            input_mappings[target_field] = {
                                '$from': {'step': source_execution_id},
                                'path': 'result'
                            }
                
                # Add input mappings if any exist
                if input_mappings:
                    execution_step['input']['input'] = input_mappings
                
                # Add depends_on for execution order
                execution_step['depends_on'] = [udf_step['id']]
                
                # Add dependencies on source components (external to the fusion chain)
                if primary_id in edge_mappings:
                    for edge in edge_mappings[primary_id]:
                        source_node = edge['source']
                        if source_node not in fusion_chain_ids:  # Only external dependencies
                            source_execution_id = self._get_fused_execution_id(source_node, udf_steps)
                            execution_step['depends_on'].append(source_execution_id)
                
                execution_steps.append(execution_step)
                
            else:
                # This is a single component UDF - handle normally
                component_type = udf_data['component_type']
                node_id = udf_step['id'].replace('create_', '').replace('_udf', '')
                
                # Skip note nodes
                if component_type in ['unknown']:
                    continue
                
                # Create execution step
                execution_step = {
                    'id': f'execute_{node_id}',
                    'component': 'langflow://udf_executor',
                    'input': {
                        'blob_id': {
                            '$from': {'step': udf_step['id']},
                            'path': 'blob_id'
                        }
                    }
                }
                
                # Add input mappings based on edges
                input_mappings = {}
                
                # Special case: ChatInput gets data from workflow input
                if component_type == 'ChatInput':
                    input_mappings['input_value'] = {
                        '$from': {'workflow': 'input'},
                        'path': 'message'
                    }
                # Check if this is an entry node (no incoming edges)
                elif node_id not in nodes_with_incoming:
                    # This is an entry node - map workflow inputs to its fields
                    # Get the node's template to understand what fields it needs
                    template = udf_data.get('template', {})
                    
                    for field_name, field_config in template.items():
                        if not isinstance(field_config, dict):
                            continue
                            
                        # Skip internal/advanced fields
                        if field_name in ['code', '_type'] or field_config.get('advanced', False):
                            continue
                            
                        # Check if this field needs external input
                        field_value = field_config.get('value')
                        is_list = field_config.get('list', False)
                        is_required = field_config.get('required', False)
                        is_shown = field_config.get('show', True)
                        
                        # Same logic as input schema generation
                        tool_mode = field_config.get('tool_mode', False)
                        
                        if is_shown and (is_required or tool_mode or 
                                        (is_list and isinstance(field_value, list)) or
                                        (field_name in ['urls', 'url', 'input', 'query', 'text', 'prompt'])):
                            # Map from workflow input
                            param_name = f"{component_type}_{field_name}".replace('-', '_')
                            input_mappings[field_name] = {
                                '$from': {'workflow': 'input'},
                                'path': param_name
                            }
                else:
                    # Map inputs based on edge connections
                    node_edges = edge_mappings.get(node_id, [])
                    for edge in node_edges:
                        source_node = edge['source']
                        target_field = edge['target_field']
                        
                        # Get the correct execution ID (handles fused components)
                        source_execution_id = self._get_fused_execution_id(source_node, udf_steps)
                        
                        input_mappings[target_field] = {
                            '$from': {'step': source_execution_id},
                            'path': 'result'
                        }
                
                # Add input mappings if any exist
                if input_mappings:
                    execution_step['input']['input'] = input_mappings
                
                # Add depends_on for execution order
                execution_step['depends_on'] = [udf_step['id']]
                
                # Add dependencies on source components
                if node_id in edge_mappings:
                    for edge in edge_mappings[node_id]:
                        source_execution_id = self._get_fused_execution_id(edge['source'], udf_steps)
                        execution_step['depends_on'].append(source_execution_id)
                
                execution_steps.append(execution_step)
        
        return execution_steps
    
    def _is_source_fused(self, source_node_id: str, udf_steps: List[Dict[str, Any]]) -> bool:
        """Check if a source node is part of a fused UDF."""
        for udf_step in udf_steps:
            udf_data = udf_step['input']['data']
            if 'fusion_chain_ids' in udf_data:
                if source_node_id in udf_data['fusion_chain_ids']:
                    return True
        return False
    
    def _is_chat_input_component(self, node_id: str, udf_steps: List[Dict[str, Any]]) -> bool:
        """Check if a node is a ChatInput component."""
        for udf_step in udf_steps:
            udf_data = udf_step['input']['data']
            # For single component UDFs
            if udf_data.get('component_type') == 'ChatInput':
                # Extract node ID from UDF step ID
                step_node_id = udf_step['id'].replace('create_', '').replace('_udf', '')
                if step_node_id == node_id:
                    return True
            # For fused UDFs, check the execution chain
            elif 'execution_chain' in udf_data:
                fusion_chain_ids = udf_data.get('fusion_chain_ids', [])
                if node_id in fusion_chain_ids:
                    execution_chain = udf_data.get('execution_chain', [])
                    # Check if this component in the chain is ChatInput
                    for i, component_type in enumerate(execution_chain):
                        if component_type == 'ChatInput' and i < len(fusion_chain_ids):
                            if fusion_chain_ids[i] == node_id:
                                return True
        return False
    
    def _get_fused_execution_id(self, source_node_id: str, udf_steps: List[Dict[str, Any]]) -> str:
        """Get the execution ID for a fused component."""
        for udf_step in udf_steps:
            udf_data = udf_step['input']['data']
            if 'fusion_chain_ids' in udf_data:
                if source_node_id in udf_data['fusion_chain_ids']:
                    # Extract the fused name from the UDF step ID
                    # UDF step ID format: create_fused_{name}_udf
                    # We want execution ID format: execute_fused_{name}
                    udf_step_id = udf_step['id']
                    if udf_step_id.startswith('create_fused_') and udf_step_id.endswith('_udf'):
                        fused_name = udf_step_id[13:-4]  # Remove 'create_fused_' and '_udf'
                        return f'execute_fused_{fused_name}'
        # Fallback to regular execution ID
        return f'execute_{source_node_id}'
    
    def _parse_edge_mappings(self, edges: List[Dict[str, Any]]) -> Dict[str, List[Dict[str, str]]]:
        """Parse Langflow edges into field-level input mappings."""
        mappings = {}
        
        for edge in edges:
            source_id = edge.get('source')
            target_id = edge.get('target')
            
            # Extract field information from edge data
            edge_data = edge.get('data', {})
            target_handle = edge_data.get('targetHandle', {})
            
            if isinstance(target_handle, dict):
                target_field = target_handle.get('fieldName')
            elif isinstance(target_handle, str):
                # Parse JSON-like string format
                import json
                try:
                    target_handle_data = target_handle.replace('œ', '"')
                    target_handle_parsed = json.loads(target_handle_data)
                    target_field = target_handle_parsed.get('fieldName')
                except:
                    target_field = 'input_value'  # fallback
            else:
                target_field = 'input_value'  # fallback
            
            if source_id and target_id and target_field:
                if target_id not in mappings:
                    mappings[target_id] = []
                
                mappings[target_id].append({
                    'source': source_id,
                    'target_field': target_field
                })
                
        
        return mappings
    
    def _sort_steps_by_dependencies(self, steps: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """Sort steps to ensure dependencies are defined before they're referenced."""
        sorted_steps = []
        step_dict = {step['id']: step for step in steps}
        processed = set()
        
        def add_step_with_deps(step_id: str):
            if step_id in processed or step_id not in step_dict:
                return
                
            step = step_dict[step_id]
            
            # First add all dependencies
            depends_on = step.get('depends_on', [])
            for dep in depends_on:
                add_step_with_deps(dep)
            
            # Check for $from references in inputs
            if 'input' in step:
                self._add_from_dependencies(step['input'], add_step_with_deps)
            
            # Add this step
            sorted_steps.append(step)
            processed.add(step_id)
        
        # Process all steps
        for step in steps:
            add_step_with_deps(step['id'])
        
        return sorted_steps
    
    def _add_from_dependencies(self, obj, add_step_func):
        """Recursively find and add $from step dependencies."""
        if isinstance(obj, dict):
            if '$from' in obj and 'step' in obj['$from']:
                add_step_func(obj['$from']['step'])
            else:
                for value in obj.values():
                    self._add_from_dependencies(value, add_step_func)
        elif isinstance(obj, list):
            for item in obj:
                self._add_from_dependencies(item, add_step_func)

    def _create_output_mapping(self, output_node: Dict[str, Any], execution_steps: List[Dict[str, Any]]) -> Dict[str, Any]:
        """Create workflow output mapping from ChatOutput node."""
        if not execution_steps:
            return {'response': 'No execution steps found'}
        
        # Find the output execution step (usually the last one or ChatOutput)
        output_step = None
        for step in execution_steps:
            if 'ChatOutput' in step['id'] or 'output' in step['id'].lower():
                output_step = step
                break
        
        if not output_step:
            output_step = execution_steps[-1]  # Use last step as fallback
        
        return {
            'response': {
                '$from': {'step': output_step['id']},
                'path': 'result'
            }
        }


def main():
    """Main CLI interface."""
    parser = argparse.ArgumentParser(
        description='Convert Langflow JSON to StepFlow YAML using UDF execution',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog='''
Examples:
  # Translate a Langflow JSON file
  python udf_translator_cli.py input.json output.yaml
  
  # Translate with pretty printing
  python udf_translator_cli.py input.json output.yaml --pretty
  
  # Analyze a Langflow JSON structure
  python udf_translator_cli.py input.json --analyze-only
        '''
    )
    
    parser.add_argument('input_file', help='Input Langflow JSON file')
    parser.add_argument('output_file', nargs='?', help='Output StepFlow YAML file')
    parser.add_argument('--pretty', action='store_true', help='Pretty print the output')
    parser.add_argument('--analyze-only', action='store_true', help='Only analyze the structure, don\'t translate')
    
    args = parser.parse_args()
    
    # Check if input file exists
    if not Path(args.input_file).exists():
        print(f"Error: Input file '{args.input_file}' not found")
        return 1
    
    # Load Langflow JSON
    try:
        with open(args.input_file, 'r') as f:
            langflow_data = json.load(f)
    except Exception as e:
        print(f"Error loading Langflow JSON: {e}")
        return 1
    
    # If analyze only, just print structure
    if args.analyze_only:
        analyze_langflow_structure(langflow_data)
        return 0
    
    # Check if output file specified
    if not args.output_file:
        print("Error: Output file required (unless using --analyze-only)")
        return 1
    
    # Create translator and translate
    translator = LangflowUDFTranslator()
    
    try:
        print(f"Translating {args.input_file} to {args.output_file}...")
        stepflow_data = translator.translate(langflow_data)
        
        # Save the result
        with open(args.output_file, 'w') as f:
            yaml.dump(stepflow_data, f, default_flow_style=False, indent=2, sort_keys=False)
        
        print(f"✅ Translation successful!")
        print(f"Output saved to: {args.output_file}")
        
        # Print statistics
        print(f"\nTranslation Statistics:")
        print(f"- Original nodes: {len(langflow_data.get('data', {}).get('nodes', []))}")
        print(f"- Translated steps: {len(stepflow_data.get('steps', []))}")
        print(f"- Has input schema: {stepflow_data.get('input_schema') is not None}")
        print(f"- Has output mapping: {stepflow_data.get('output') is not None}")
        
        if args.pretty:
            print(f"\n=== Generated StepFlow YAML ===")
            print(yaml.dump(stepflow_data, default_flow_style=False, indent=2))
        
        return 0
        
    except Exception as e:
        print(f"❌ Translation failed: {e}")
        import traceback
        traceback.print_exc()
        return 1

def analyze_langflow_structure(langflow_data):
    """Analyze the Langflow JSON structure."""
    
    nodes = langflow_data.get('data', {}).get('nodes', [])
    edges = langflow_data.get('data', {}).get('edges', [])
    
    print("=== Langflow Structure Analysis ===")
    print(f"Workflow name: {langflow_data.get('name', 'Unnamed')}")
    print(f"Total nodes: {len(nodes)}")
    print(f"Total edges: {len(edges)}")
    
    print("\nNode Details:")
    for i, node in enumerate(nodes, 1):
        node_type = node.get('data', {}).get('type', 'Unknown')
        node_id = node.get('id', 'Unknown')
        
        # Check if it has code
        template = node.get('data', {}).get('node', {}).get('template', {})
        has_code = 'code' in template and template.get('code', {}).get('value', '') != ''
        
        # Check output types
        outputs = node.get('data', {}).get('node', {}).get('outputs', [])
        output_types = outputs[0].get('types', []) if outputs else []
        
        # Check base classes
        base_classes = node.get('data', {}).get('node', {}).get('base_classes', [])
        
        print(f"  {i}. {node_type} (ID: {node_id})")
        print(f"     Has code: {has_code}")
        print(f"     Output types: {output_types}")
        print(f"     Base classes: {base_classes}")
        
        if has_code:
            code_lines = template.get('code', {}).get('value', '').split('\n')
            print(f"     Code lines: {len(code_lines)}")
            
            # Show class definition if found
            for line in code_lines:
                if line.strip().startswith('class '):
                    class_name = line.strip().split('class ')[1].split('(')[0].split(':')[0]
                    print(f"     Class: {class_name}")
                    break
        print()
    
    print("Edge Connections:")
    for i, edge in enumerate(edges, 1):
        source_id = edge.get('source', 'Unknown')
        target_id = edge.get('target', 'Unknown')
        
        # Get source and target handles
        source_handle = edge.get('data', {}).get('sourceHandle', {})
        target_handle = edge.get('data', {}).get('targetHandle', {})
        
        source_name = source_handle.get('name', 'unknown') if isinstance(source_handle, dict) else 'unknown'
        target_field = target_handle.get('fieldName', 'unknown') if isinstance(target_handle, dict) else 'unknown'
        
        print(f"  {i}. {source_id}[{source_name}] → {target_id}[{target_field}]")

if __name__ == "__main__":
    exit(main()) 