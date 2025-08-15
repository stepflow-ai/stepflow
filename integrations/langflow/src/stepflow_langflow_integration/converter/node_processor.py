"""Process individual Langflow nodes into Stepflow steps."""

import json
from typing import Dict, Any, List, Optional

from ..types.stepflow import StepflowStep 
from ..utils.errors import ConversionError
from .schema_mapper import SchemaMapper


class NodeProcessor:
    """Processes individual Langflow nodes into Stepflow steps."""
    
    def __init__(self):
        """Initialize node processor."""
        self.schema_mapper = SchemaMapper()
    
    def process_node(
        self, 
        node: Dict[str, Any], 
        dependencies: Dict[str, List[str]]
    ) -> Optional[StepflowStep]:
        """Process a Langflow node into a Stepflow step.
        
        Args:
            node: Langflow node object
            dependencies: Dependency graph for all nodes
            
        Returns:
            StepflowStep object or None if node should be skipped
        """
        try:
            node_id = node.get("id")
            if not node_id:
                raise ConversionError("Node missing ID")
            
            # Check if this is a note or documentation node that should be skipped
            node_type = node.get("type", "")
            if node_type == "noteNode":
                # This is a React Flow note node - skip it entirely
                return None
            
            node_data = node.get("data", {})
            component_type = node_data.get("type", "")
            
            # Skip nodes without a valid component type (documentation nodes, etc.)
            if not component_type or component_type.strip() == "":
                return None
            
            # Generate step ID (clean up for Stepflow)
            step_id = self._generate_step_id(node_id, component_type)
            
            # Check if this is a custom component (has code) or built-in
            node_data = node.get("data", {})
            node_info = node_data.get("node", {})
            template = node_info.get("template", {})
            custom_code = template.get("code", {}).get("value", "")
            
            if custom_code:
                # Custom component - use UDF executor
                blob_data = self._prepare_udf_blob(node, component_type)
                
                step_input = {
                    "blob_id": f"udf_{step_id}",  # Will be replaced with actual blob ID
                    "input": self._extract_runtime_inputs(node, dependencies.get(node_id, []))
                }
                
                component_path = "/langflow/udf_executor"
                
                # Store blob data for later storage
                blob_data_to_store = blob_data
            else:
                # Built-in component - use Langflow component directly
                step_input = self._extract_component_inputs(node, dependencies.get(node_id, []))
                component_path = f"/langflow/{component_type}"
                blob_data_to_store = None
            
            # Extract output schema
            output_schema = self.schema_mapper.extract_output_schema(node)
            
            # Create Stepflow step
            step = StepflowStep(
                id=step_id,
                component=component_path,
                input=step_input,
                output_schema=output_schema,
                # Dependencies will be handled at the workflow level
            )
            
            # Store blob data for later storage (only for UDF components)
            if blob_data_to_store:
                step._udf_blob_data = blob_data_to_store  # type: ignore
            
            return step
            
        except Exception as e:
            raise ConversionError(f"Error processing node {node.get('id', 'unknown')}: {e}")
    
    def _generate_step_id(self, node_id: str, component_type: str) -> str:
        """Generate a clean step ID from node ID and type.
        
        Args:
            node_id: Original Langflow node ID
            component_type: Component type name
            
        Returns:
            Clean step ID suitable for Stepflow
        """
        # Always use the full node_id to ensure uniqueness
        # For any node_id with a suffix, keep it to ensure uniqueness
        base_id = node_id.lower()
        
        # Always use langflow prefix with the full base_id to guarantee uniqueness
        return f"langflow_{base_id}"
    
    def _prepare_udf_blob(self, node: Dict[str, Any], component_type: str) -> Dict[str, Any]:
        """Prepare UDF blob data for component execution.
        
        Args:
            node: Langflow node object
            component_type: Component type name
            
        Returns:
            UDF blob data
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        
        # Extract component code
        template = node_info.get("template", {})
        code = template.get("code", {}).get("value", "")
        
        if not code:
            raise ConversionError(f"No code found for component {component_type}")
        
        # Extract outputs information
        outputs = node_data.get("outputs", [])
        selected_output = None
        if outputs:
            # For now, use the first output
            selected_output = outputs[0].get("name")
        
        # Prepare template (remove code field to avoid duplication)
        prepared_template = {}
        for field_name, field_config in template.items():
            if field_name != "code":
                prepared_template[field_name] = field_config
        
        return {
            "code": code,
            "template": prepared_template,
            "component_type": component_type,
            "outputs": outputs,
            "selected_output": selected_output,
        }
    
    def _extract_runtime_inputs(
        self, 
        node: Dict[str, Any], 
        dependency_node_ids: List[str]
    ) -> Dict[str, Any]:
        """Extract runtime inputs that will come from other workflow steps.
        
        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on
            
        Returns:
            Dict of runtime inputs
        """
        runtime_inputs = {}
        
        # For now, create placeholder inputs based on dependencies
        # TODO: Map actual edge connections to specific input fields
        for i, dep_id in enumerate(dependency_node_ids):
            dep_step_id = self._generate_step_id(dep_id, "")
            runtime_inputs[f"input_{i}"] = {
                "$from": {"step": dep_step_id},
                "path": "result"
            }
        
        return runtime_inputs
    
    def _extract_component_inputs(
        self, 
        node: Dict[str, Any], 
        dependency_node_ids: List[str]
    ) -> Dict[str, Any]:
        """Extract inputs for built-in Langflow components.
        
        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on
            
        Returns:
            Dict of component inputs mapped to template fields
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        template = node_info.get("template", {})
        
        component_inputs = {}
        
        # Map static template values to component inputs
        for field_name, field_config in template.items():
            if isinstance(field_config, dict):
                field_value = field_config.get("value")
                if field_value is not None:
                    component_inputs[field_name] = field_value
        
        # Map dependencies to component inputs based on edges
        # For now, use a simple mapping - this should be improved with actual edge analysis
        if dependency_node_ids:
            # Map first dependency to common input field names
            dep_step_id = self._generate_step_id(dependency_node_ids[0], "")
            
            # Common input field names for different component types
            component_type = node_data.get("type", "")
            if component_type in ["ChatOutput", "TextOutput"]:
                component_inputs["input_value"] = {
                    "$from": {"step": dep_step_id},
                    "path": "result"
                }
            elif component_type in ["LanguageModelComponent", "OpenAIModelComponent"]:
                component_inputs["input_value"] = {
                    "$from": {"step": dep_step_id},
                    "path": "result"
                }
            else:
                # Generic mapping
                component_inputs["input"] = {
                    "$from": {"step": dep_step_id},
                    "path": "result"
                }
        
        return component_inputs