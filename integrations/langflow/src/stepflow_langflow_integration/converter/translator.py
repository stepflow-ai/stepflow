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

"""Main Langflow to Stepflow converter implementation."""

import json
import yaml
from pathlib import Path
from typing import Dict, Any, List, Optional, Union
from dataclasses import dataclass

from stepflow_py import FlowBuilder, Value

from ..types.stepflow import StepflowWorkflow, StepflowStep
from ..utils.errors import ConversionError, ValidationError
from .dependency_analyzer import DependencyAnalyzer
from .schema_mapper import SchemaMapper
from .node_processor import NodeProcessor


@dataclass
class WorkflowAnalysis:
    """Typed analysis results from analyzing a Langflow workflow."""
    
    node_count: int  # Total number of nodes in the Langflow workflow
    edge_count: int  # Total number of connections/edges between nodes
    component_types: Dict[str, int]  # Map of component type names to their counts (e.g., {"ChatInput": 1, "OpenAI": 2})
    dependencies: Dict[str, List[str]]  # Map of node IDs to lists of their dependency node IDs
    potential_issues: List[str]  # List of warnings or potential problems detected during analysis


class LangflowConverter:
    """Convert Langflow JSON workflows to Stepflow YAML workflows."""
    
    def __init__(self, validate_schemas: bool = False):
        """Initialize the converter.
        
        Args:
            validate_schemas: Whether to perform additional schema validation
        """
        self.validate_schemas = validate_schemas
        self.dependency_analyzer = DependencyAnalyzer()
        self.schema_mapper = SchemaMapper()
        self.node_processor = NodeProcessor()
    
    def convert_file(self, input_path: Union[str, Path]) -> str:
        """Convert a Langflow JSON file to Stepflow YAML.
        
        Args:
            input_path: Path to the Langflow JSON file
            
        Returns:
            Stepflow YAML as a string
            
        Raises:
            ConversionError: If conversion fails
            ValidationError: If validation fails
        """
        input_path = Path(input_path)
        if not input_path.exists():
            raise ConversionError(f"Input file not found: {input_path}")
        
        try:
            with open(input_path, "r", encoding="utf-8") as f:
                langflow_data = json.load(f)
        except json.JSONDecodeError as e:
            raise ConversionError(f"Invalid JSON in {input_path}: {e}")
        except Exception as e:
            raise ConversionError(f"Error reading {input_path}: {e}")
        
        workflow = self.convert(langflow_data)
        return self.to_yaml(workflow)
    
    def convert(self, langflow_data: Dict[str, Any]) -> StepflowWorkflow:
        """Convert Langflow data structure to Stepflow workflow.
        
        Args:
            langflow_data: Parsed Langflow JSON data
            
        Returns:
            StepflowWorkflow object
            
        Raises:
            ConversionError: If conversion fails
        """
        try:
            # Extract main data structure
            if "data" not in langflow_data:
                raise ConversionError("Invalid Langflow JSON: missing 'data' key")
            
            data = langflow_data["data"]
            nodes = data.get("nodes", [])
            edges = data.get("edges", [])
            
            if not nodes:
                raise ConversionError("No nodes found in Langflow workflow")
            
            # Build dependency graph from edges
            dependencies = self.dependency_analyzer.build_dependency_graph(edges)
            
            # Get proper execution order using topological sort
            execution_order = self.dependency_analyzer.get_execution_order(dependencies)
            
            # Create field mapping from edges for proper UDF input handling
            field_mapping = self._build_field_mapping_from_edges(edges)
            
            # Apply complex configuration transformations (merge embeddings into vector stores)
            nodes, edges, dependencies = self._apply_complex_configuration_transforms(nodes, edges, dependencies)
            
            # Create node lookup for efficient processing
            node_lookup = {node["id"]: node for node in nodes}
            
            # Create FlowBuilder with namespace for unique step IDs
            builder = FlowBuilder(
                name=self._generate_workflow_name(langflow_data),
                namespace="langflow"
            )
            
            # Note: Skip setting input schema for now as Schema class doesn't support properties
            # input_schema = self._generate_input_section(nodes)
            # if input_schema:
            #     builder.set_input_schema(input_schema)
            
            # Process nodes and collect output references
            node_output_refs = {}  # node_id -> output reference
            processed_nodes = set()
            
            # First, process nodes in execution order
            for node_id in execution_order:
                if node_id in node_lookup:
                    output_ref = self.node_processor.process_node(node_lookup[node_id], dependencies, nodes, builder, node_output_refs, field_mapping)
                    if output_ref is not None:
                        node_output_refs[node_id] = output_ref
                    processed_nodes.add(node_id)
            
            # Then, process any remaining nodes (nodes with no dependencies)
            for node in nodes:
                node_id = node["id"]
                if node_id not in processed_nodes:
                    output_ref = self.node_processor.process_node(node, dependencies, nodes, builder, node_output_refs, field_mapping)
                    if output_ref is not None:
                        node_output_refs[node_id] = output_ref
            
            # Set workflow output using incremental output building
            self._build_flow_output(builder, nodes, dependencies, node_output_refs)
            
            # Build the flow using stepflow_py
            flow = builder.build()
            
            # Convert to our legacy StepflowWorkflow format for compatibility
            workflow = self._convert_flow_to_stepflow_workflow(flow)
            
            # Optional validation
            if self.validate_schemas:
                self._validate_workflow(workflow)
            
            return workflow
            
        except ConversionError:
            raise
        except Exception as e:
            import traceback
            print(f"Full traceback: {traceback.format_exc()}")
            raise ConversionError(f"Unexpected error during conversion: {e}")
    
    def to_yaml(self, workflow: StepflowWorkflow) -> str:
        """Convert StepflowWorkflow to YAML string.
        
        Args:
            workflow: StepflowWorkflow object
            
        Returns:
            YAML string
        """
        try:
            # Convert to dict representation
            workflow_dict = workflow.to_dict()
            
            # Generate clean YAML
            return yaml.dump(
                workflow_dict,
                default_flow_style=False,
                sort_keys=False,
                allow_unicode=True,
                width=120,
            )
        except Exception as e:
            raise ConversionError(f"Error generating YAML: {e}")
    
    def analyze(self, langflow_data: Dict[str, Any]) -> WorkflowAnalysis:
        """Analyze Langflow workflow structure without conversion.
        
        Args:
            langflow_data: Parsed Langflow JSON data
            
        Returns:
            Typed analysis results
        """
        try:
            data = langflow_data.get("data", {})
            nodes = data.get("nodes", [])
            edges = data.get("edges", [])
            
            # Basic statistics
            analysis = {
                "node_count": len(nodes),
                "edge_count": len(edges),
                "component_types": {},
                "dependencies": {},
                "potential_issues": [],
            }
            
            # Analyze nodes
            for node in nodes:
                node_data = node.get("data", {})
                component_type = node_data.get("type", "Unknown")
                
                if component_type not in analysis["component_types"]:
                    analysis["component_types"][component_type] = 0
                analysis["component_types"][component_type] += 1
                
                # Check for potential issues
                if not node.get("id"):
                    analysis["potential_issues"].append("Node missing ID")
                if not node_data.get("node", {}).get("template"):
                    analysis["potential_issues"].append(
                        f"Node {node.get('id', 'unknown')} missing template"
                    )
            
            # Analyze dependencies
            dependencies = self.dependency_analyzer.build_dependency_graph(edges)
            
            return WorkflowAnalysis(
                node_count=len(nodes),
                edge_count=len(edges),
                component_types=analysis["component_types"],
                dependencies=dependencies,
                potential_issues=analysis["potential_issues"]
            )
            
        except Exception as e:
            raise ConversionError(f"Error analyzing workflow: {e}")
    
    def _generate_workflow_name(self, langflow_data: Dict[str, Any]) -> str:
        """Generate a workflow name from Langflow data."""
        # Try to get name from various sources
        if "name" in langflow_data:
            return langflow_data["name"]
        
        data = langflow_data.get("data", {})
        if "name" in data:
            return data["name"]
        
        # Fallback to generic name
        return "Converted Langflow Workflow"
    
    def _build_field_mapping_from_edges(self, edges: List[Dict[str, Any]]) -> Dict[str, Dict[str, str]]:
        """Build field mapping from edges for proper input handling.
        
        Args:
            edges: List of Langflow edges
            
        Returns:
            Dict mapping target_node_id -> {source_node_id -> target_field_name}
        """
        field_mapping = {}
        
        for edge in edges:
            target_id = edge.get("target")
            source_id = edge.get("source")
            
            if not target_id or not source_id:
                continue
                
            # Get target field name from edge data
            edge_data = edge.get("data", {})
            target_handle = edge_data.get("targetHandle", {})
            
            if isinstance(target_handle, dict):
                field_name = target_handle.get("fieldName")
            elif isinstance(target_handle, str):
                # Sometimes targetHandle is a JSON string - handle this case
                try:
                    import json
                    target_info = json.loads(target_handle.replace("œ", "\""))
                    field_name = target_info.get("fieldName")
                except:
                    field_name = None
            else:
                field_name = None
            
            if field_name:
                if target_id not in field_mapping:
                    field_mapping[target_id] = {}
                field_mapping[target_id][source_id] = field_name
        
        return field_mapping
    
    def _apply_complex_configuration_transforms(self, nodes: List[Dict[str, Any]], edges: List[Dict[str, Any]], dependencies: Dict[str, List[str]]) -> tuple[List[Dict[str, Any]], List[Dict[str, Any]], Dict[str, List[str]]]:
        """Apply complex configuration transformations to merge embeddings into vector stores.
        
        Args:
            nodes: List of Langflow nodes
            edges: List of Langflow edges  
            dependencies: Node dependency graph
            
        Returns:
            Tuple of (transformed_nodes, transformed_edges, transformed_dependencies)
        """
        # Find embedding components and their target vector stores
        embedding_to_vector_store = {}  # embedding_node_id -> vector_store_node_id
        nodes_to_remove = set()
        edges_to_remove = set()
        
        # Build a mapping of embedding connections
        for edge in edges:
            source_id = edge.get("source")
            target_id = edge.get("target")
            
            if not source_id or not target_id:
                continue
            
            # Find the source and target nodes
            source_node = next((n for n in nodes if n["id"] == source_id), None)
            target_node = next((n for n in nodes if n["id"] == target_id), None)
            
            if not source_node or not target_node:
                continue
                
            source_type = source_node.get("data", {}).get("type", "")
            target_type = target_node.get("data", {}).get("type", "")
            
            # Debug: Log what we're finding
            if source_type == "OpenAIEmbeddings":
                print(f"DEBUG Transform: Found OpenAIEmbeddings {source_id} -> {target_type} {target_id}")
            
            # Check for embedding -> vector store connection
            if source_type == "OpenAIEmbeddings" and target_type in ["AstraDB", "VectorStore", "Chroma", "FAISS", "Pinecone"]:
                # Get the field name from edge data
                edge_data = edge.get("data", {})
                target_handle = edge_data.get("targetHandle", {})
                
                field_name = None
                if isinstance(target_handle, dict):
                    field_name = target_handle.get("fieldName")
                elif isinstance(target_handle, str):
                    try:
                        import json
                        target_info = json.loads(target_handle.replace("œ", "\""))
                        field_name = target_info.get("fieldName")
                    except:
                        pass
                
                print(f"DEBUG Transform: Field name for {source_id} -> {target_id}: {field_name}")
                
                # Check if this is an embedding_model or similar field
                if field_name and "embedding" in field_name.lower():
                    print(f"DEBUG Transform: Merging {source_id} into {target_id} field {field_name}")
                    embedding_to_vector_store[source_id] = target_id
                    nodes_to_remove.add(source_id)
                    edges_to_remove.add(edge.get("id"))
                    
                    # Merge embedding configuration into vector store
                    self._merge_embedding_config(source_node, target_node, field_name)
                else:
                    print(f"DEBUG Transform: Skipping {source_id} -> {target_id}, field '{field_name}' doesn't contain 'embedding'")
        
        # Remove embedding nodes and edges
        filtered_nodes = [n for n in nodes if n["id"] not in nodes_to_remove]
        filtered_edges = [e for e in edges if e.get("id") not in edges_to_remove]
        
        # Debug: Show what was removed
        if nodes_to_remove:
            print(f"DEBUG Transform: Removed {len(nodes_to_remove)} embedding nodes: {list(nodes_to_remove)}")
            print(f"DEBUG Transform: Remaining nodes: {[n['id'] for n in filtered_nodes]}")
        
        # Update dependencies to remove embedding nodes
        filtered_dependencies = {}
        for node_id, deps in dependencies.items():
            if node_id not in nodes_to_remove:
                # Filter out dependencies on removed embedding nodes
                filtered_deps = [dep for dep in deps if dep not in nodes_to_remove]
                filtered_dependencies[node_id] = filtered_deps
        
        return filtered_nodes, filtered_edges, filtered_dependencies
    
    def _merge_embedding_config(self, embedding_node: Dict[str, Any], vector_store_node: Dict[str, Any], field_name: str):
        """Merge embedding configuration into vector store node.
        
        Args:
            embedding_node: OpenAI embeddings node to merge
            vector_store_node: Vector store node to merge into
            field_name: Name of the embedding field in vector store
        """
        # Get embedding configuration from node template
        embedding_data = embedding_node.get("data", {})
        embedding_template = embedding_data.get("node", {}).get("template", {})
        
        # Get vector store configuration
        vector_store_data = vector_store_node.get("data", {})
        if "node" not in vector_store_data:
            vector_store_data["node"] = {}
        if "template" not in vector_store_data["node"]:
            vector_store_data["node"]["template"] = {}
        
        vector_store_template = vector_store_data["node"]["template"]
        
        # Create complex embedding configuration object
        embedding_config = {
            "type": "dict",
            "value": {
                "component_type": "OpenAIEmbeddings",
                "config": {}
            },
            "info": f"Embedded OpenAI Embeddings configuration for {field_name}"
        }
        
        # Copy relevant embedding parameters
        for param_name, param_config in embedding_template.items():
            if param_name in ["api_key", "model", "openai_api_key", "dimensions", "chunk_size", "max_retries"]:
                embedding_config["value"]["config"][param_name] = param_config.get("value")
        
        # Set the embedding configuration in the vector store template
        vector_store_template[f"_embedding_config_{field_name}"] = embedding_config
        
        # Debug: Show what was merged
        print(f"DEBUG Transform: Added _embedding_config_{field_name} to {vector_store_node['id']}")
        print(f"DEBUG Transform: Embedding config: {embedding_config}")
        
        # Mark this vector store as having embedded configuration
        # This will be used by the node processor to route to standalone server
        vector_store_template["_has_embedded_config"] = {
            "type": "bool",
            "value": True,
            "info": "Indicates this component has embedded configuration and should use standalone server"
        }
        
        # Also preserve the original field for backward compatibility but mark it as configured
        if field_name in vector_store_template:
            vector_store_template[field_name]["_configured_by_embedding"] = True
    
    def _generate_input_section(self, nodes: List[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
        """Generate input section for workflow based on ChatInput components.
        
        Args:
            nodes: List of Langflow nodes
            
        Returns:
            Input section dict or None if no ChatInput components found
        """
        # Look for ChatInput components
        chat_input_nodes = []
        for node in nodes:
            node_data = node.get("data", {})
            component_type = node_data.get("type", "")
            if component_type == "ChatInput":
                chat_input_nodes.append(node)
        
        if not chat_input_nodes:
            return None
        
        # Generate input schema for ChatInput components
        # For now, assume all ChatInput components expect a "message" field
        input_schema = {
            "message": {
                "type": "string",
                "description": "Message input for the chat workflow"
            }
        }
        
        # If multiple ChatInput components, we might need more complex input schema
        if len(chat_input_nodes) > 1:
            # For multiple inputs, create named fields based on node IDs
            input_schema = {}
            for node in chat_input_nodes:
                node_id = node.get("id", "")
                field_name = f"message_{node_id.lower().replace('-', '_')}"
                input_schema[field_name] = {
                    "type": "string", 
                    "description": f"Message input for {node_id}"
                }
        
        return input_schema
    
    def _generate_output_section(self, steps: List[StepflowStep], dependencies: Dict[str, List[str]], nodes: List[Dict[str, Any]] = None) -> Optional[Dict[str, Any]]:
        """Generate output section for workflow based on step types and dependencies.
        
        Args:
            steps: List of workflow steps
            dependencies: Dependency graph
            nodes: Original Langflow nodes (for handling no-step workflows)
            
        Returns:
            Output section dict or None if no obvious output step
        """
        if not steps:
            # Handle workflows with no steps (ChatInput → ChatOutput)
            if nodes:
                chat_input_nodes = [n for n in nodes if n.get("data", {}).get("type") == "ChatInput"]
                chat_output_nodes = [n for n in nodes if n.get("data", {}).get("type") == "ChatOutput"]
                
                if chat_input_nodes and chat_output_nodes:
                    # Direct passthrough from input to output
                    return {
                        "$from": {"workflow": "input"},
                        "path": "message"
                    }
            return None
        
        # Find output steps (steps with no dependents or known output types)
        step_ids = {step.id for step in steps}
        output_steps = []
        
        # Find steps that nothing else depends on (leaf nodes)
        dependent_steps = set()
        for step_id, deps in dependencies.items():
            dependent_steps.update(deps)
        
        leaf_steps = [step for step in steps if step.id not in dependent_steps]
        
        # First, look for ChatOutput components specifically (highest priority)
        for step in steps:
            component_lower = step.component.lower()
            step_id_lower = step.id.lower()
            # Check if this is a ChatOutput component (either direct or UDF)
            if ('chatoutput' in step_id_lower or 'chat_output' in step_id_lower):
                output_steps.append(step)
        
        # If no ChatOutput found, look for other output component types
        if not output_steps:
            for step in leaf_steps:
                component_lower = step.component.lower()
                if any(output_type in component_lower for output_type in ['output']):
                    output_steps.append(step)
                # Also prioritize steps that were originally output components (now using identity)
                elif step.component == "/builtin/identity":
                    output_steps.append(step)
        
        # If no specific output components, use all leaf steps
        if not output_steps:
            output_steps = leaf_steps
        
        # If still no output steps, use the last step
        if not output_steps and steps:
            output_steps = [steps[-1]]
        
        # Generate output section
        if len(output_steps) == 1:
            # Single output step - return its result directly
            step = output_steps[0]
            return {
                "$from": {"step": step.id},
                "path": "result"
            }
        elif len(output_steps) > 1:
            # Multiple output steps - create a structured result
            result = {}
            for step in output_steps:
                # Use a cleaned version of step ID as the key
                key = step.id.replace('-', '_').lower()
                if 'output' in key:
                    key = 'result'  # Simplify output step names
                elif 'chat' in key:
                    key = 'message'
                
                result[key] = {
                    "$from": {"step": step.id},
                    "path": "result"
                }
            return result
        
        return None
    
    def _build_flow_output(self, builder: FlowBuilder, nodes: List[Dict[str, Any]], dependencies: Dict[str, List[str]], node_output_refs: Dict[str, Any]) -> None:
        """Build workflow output using incremental output building API.
        
        Args:
            builder: FlowBuilder instance to add output fields to
            nodes: Original Langflow nodes
            dependencies: Dependency graph
            node_output_refs: Mapping of node IDs to their output references
        """
        # Look for ChatOutput nodes first
        chat_output_nodes = [n for n in nodes if n.get("data", {}).get("type") == "ChatOutput"]
        
        if chat_output_nodes:
            # Use the first ChatOutput node
            chat_output_node = chat_output_nodes[0]
            node_id = chat_output_node["id"]
            
            # Check if ChatOutput has dependencies
            if node_id in dependencies and dependencies[node_id]:
                # ChatOutput depends on another node - use that node's output
                dep_node_id = dependencies[node_id][0]
                if dep_node_id in node_output_refs:
                    builder.set_output(node_output_refs[dep_node_id])
                    return
            
            # ChatOutput has no dependencies or dependencies not found - check if it's a simple passthrough
            if chat_output_nodes and len(nodes) <= 2:
                # Simple ChatInput -> ChatOutput workflow
                chat_input_nodes = [n for n in nodes if n.get("data", {}).get("type") == "ChatInput"]
                if chat_input_nodes:
                    builder.set_output(Value.input.add_path("message"))
                    return
        
        # Find leaf nodes (nodes with no dependents)
        dependent_nodes = set()
        for deps in dependencies.values():
            dependent_nodes.update(deps)
        
        leaf_nodes = []
        for node in nodes:
            node_id = node["id"]
            component_type = node.get("data", {}).get("type", "")
            
            # Skip ChatInput/ChatOutput as they're handled specially
            if component_type in ["ChatInput", "ChatOutput"]:
                continue
                
            if node_id not in dependent_nodes and node_id in node_output_refs:
                leaf_nodes.append((node_id, component_type))
        
        # Build structured output based on leaf nodes
        if len(leaf_nodes) == 1:
            # Single leaf node - use it directly  
            node_id, _ = leaf_nodes[0]
            builder.set_output(node_output_refs[node_id])
        elif len(leaf_nodes) > 1:
            # Multiple leaf nodes - create structured output using incremental building
            for node_id, component_type in leaf_nodes:
                # Generate a clean field name from the component type
                field_name = component_type.lower().replace("component", "").replace("_", "")
                if not field_name:
                    field_name = node_id.lower().replace("-", "_")
                
                builder.add_output_field(field_name, node_output_refs[node_id])
        else:
            # No leaf nodes found - fallback to direct input passthrough
            builder.set_output(Value.input.add_path("message"))
    
    def _generate_flow_output(self, nodes: List[Dict[str, Any]], dependencies: Dict[str, List[str]], node_output_refs: Dict[str, Any]) -> Any:
        """Generate flow output using the new architecture.
        
        Args:
            nodes: Original Langflow nodes
            dependencies: Dependency graph
            node_output_refs: Mapping of node IDs to their output references
            
        Returns:
            Value for the flow output
        """
        # Look for ChatOutput nodes first
        chat_output_nodes = [n for n in nodes if n.get("data", {}).get("type") == "ChatOutput"]
        
        if chat_output_nodes:
            # Use the first ChatOutput node
            chat_output_node = chat_output_nodes[0]
            node_id = chat_output_node["id"]
            
            # Check if ChatOutput has dependencies
            if node_id in dependencies and dependencies[node_id]:
                # ChatOutput depends on another node - use that node's output
                dep_node_id = dependencies[node_id][0]
                if dep_node_id in node_output_refs:
                    return node_output_refs[dep_node_id]
            
            # ChatOutput has no dependencies or dependencies not found - check if it's a simple passthrough
            if chat_output_nodes and len(nodes) <= 2:
                # Simple ChatInput -> ChatOutput workflow
                chat_input_nodes = [n for n in nodes if n.get("data", {}).get("type") == "ChatInput"]
                if chat_input_nodes:
                    return Value.input("$.message")
        
        # Find leaf nodes (nodes with no dependents)
        dependent_nodes = set()
        for deps in dependencies.values():
            dependent_nodes.update(deps)
        
        leaf_nodes = []
        for node in nodes:
            node_id = node["id"]
            component_type = node.get("data", {}).get("type", "")
            
            # Skip ChatInput/ChatOutput as they're handled specially
            if component_type in ["ChatInput", "ChatOutput"]:
                continue
                
            if node_id not in dependent_nodes and node_id in node_output_refs:
                leaf_nodes.append(node_id)
        
        # If we have leaf nodes, use the first one
        if leaf_nodes:
            return node_output_refs[leaf_nodes[0]]
        
        # Fallback - use the last node with an output reference
        if node_output_refs:
            last_node_id = list(node_output_refs.keys())[-1]
            return node_output_refs[last_node_id]
        
        # Final fallback - direct input passthrough
        return Value.input("$.message")
    
    def _convert_flow_to_stepflow_workflow(self, flow) -> StepflowWorkflow:
        """Convert stepflow_py Flow to legacy StepflowWorkflow for compatibility.
        
        Args:
            flow: stepflow_py Flow object
            
        Returns:
            StepflowWorkflow object
        """
        # Convert steps
        steps = []
        if flow.steps:
            for step in flow.steps:
                stepflow_step = StepflowStep(
                    id=step.id,
                    component=step.component,
                    input=self._convert_value_template_to_dict(step.input) if step.input else {},
                    output_schema=self._convert_schema_to_dict(step.outputSchema) if step.outputSchema else None
                )
                steps.append(stepflow_step)
        
        # Convert input schema
        input_section = None
        if flow.inputSchema and flow.inputSchema.properties:
            input_section = flow.inputSchema.properties
        
        # Convert output
        output_section = self._convert_value_template_to_dict(flow.output) if flow.output else None
        
        return StepflowWorkflow(
            name=flow.name or "Converted Langflow Workflow",
            steps=steps,
            input=input_section,
            output=output_section
        )
    
    def _convert_value_template_to_dict(self, value_template) -> Any:
        """Convert stepflow_py ValueTemplate to dict format."""
        if value_template is None:
            return None
        
        # Handle Reference objects (they have field_from and path attributes)
        if hasattr(value_template, 'field_from'):
            result = {"$from": {}}
            
            # Handle the reference source
            if hasattr(value_template.field_from, 'workflow'):
                # Convert WorkflowRef enum to string
                workflow_ref = value_template.field_from.workflow
                if hasattr(workflow_ref, 'value'):
                    result["$from"]["workflow"] = workflow_ref.value
                else:
                    result["$from"]["workflow"] = str(workflow_ref)
            elif hasattr(value_template.field_from, 'step'):
                result["$from"]["step"] = value_template.field_from.step
            
            # Handle the path
            if value_template.path:
                result["path"] = value_template.path
            
            return result
        
        # Handle EscapedLiteral objects
        if hasattr(value_template, 'field_literal'):
            return value_template.field_literal
        
        # Handle dictionaries
        if isinstance(value_template, dict):
            result = {}
            for key, value in value_template.items():
                result[key] = self._convert_value_template_to_dict(value)
            return result
        
        # Handle lists
        if isinstance(value_template, list):
            return [self._convert_value_template_to_dict(item) for item in value_template]
        
        # Return primitive values as-is
        return value_template
    
    def _convert_schema_to_dict(self, schema) -> Dict[str, Any]:
        """Convert stepflow_py Schema to dict format."""
        if schema is None:
            return None
        
        result = {}
        if hasattr(schema, 'type') and schema.type:
            result["type"] = schema.type
        if hasattr(schema, 'properties') and schema.properties:
            result["properties"] = schema.properties
        if hasattr(schema, 'description') and schema.description:
            result["description"] = schema.description
            
        return result if result else None
    
    def _create_blob_creation_step(self, udf_step: StepflowStep) -> StepflowStep:
        """Create a blob creation step for a UDF component step.
        
        Args:
            udf_step: Step that needs blob data created
            
        Returns:
            Step that creates the blob and stores it
        """
        blob_data = getattr(udf_step, '_udf_blob_data')
        blob_step_id = f"blob_{udf_step.id}"
        
        # Create step that stores blob data using /builtin/put_blob
        blob_step = StepflowStep(
            id=blob_step_id,
            component="/builtin/put_blob",
            input={
                "data": blob_data,  # Store the blob data as a literal
                "blob_type": "data"  # Use "data" blob type for component data
            },
            output_schema={
                "type": "object",
                "properties": {
                    "blob_id": {"type": "string"}
                }
            }
        )
        
        # Update the UDF step to reference the blob creation step
        if isinstance(udf_step.input, dict) and "blob_id" in udf_step.input:
            udf_step.input["blob_id"] = {
                "$from": {"step": blob_step_id},
                "path": "blob_id"
            }
        
        return blob_step
    
    def _validate_workflow(self, workflow: StepflowWorkflow) -> None:
        """Validate the converted workflow.
        
        Args:
            workflow: StepflowWorkflow to validate
            
        Raises:
            ValidationError: If validation fails
        """
        if not workflow.steps:
            raise ValidationError("Workflow has no steps")
        
        step_ids = {step.id for step in workflow.steps}
        
        # Check for duplicate step IDs
        if len(step_ids) != len(workflow.steps):
            raise ValidationError("Duplicate step IDs found")
        
        # Check step dependencies reference valid steps
        for step in workflow.steps:
            # This would need to be implemented based on the actual
            # Stepflow step dependency format
            pass