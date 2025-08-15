"""Main Langflow to Stepflow converter implementation."""

import json
import yaml
from pathlib import Path
from typing import Dict, Any, List, Optional, Union

from ..types.stepflow import StepflowWorkflow, StepflowStep
from ..utils.errors import ConversionError, ValidationError
from .dependency_analyzer import DependencyAnalyzer
from .schema_mapper import SchemaMapper
from .node_processor import NodeProcessor


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
            
            # Convert each node to a Stepflow step
            steps = []
            for node in nodes:
                step = self.node_processor.process_node(node, dependencies)
                if step:
                    steps.append(step)
            
            if not steps:
                raise ConversionError("No valid steps generated from Langflow nodes")
            
            # Create workflow
            workflow = StepflowWorkflow(
                name=self._generate_workflow_name(langflow_data),
                steps=steps
            )
            
            # Optional validation
            if self.validate_schemas:
                self._validate_workflow(workflow)
            
            return workflow
            
        except ConversionError:
            raise
        except Exception as e:
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
    
    def analyze(self, langflow_data: Dict[str, Any]) -> Dict[str, Any]:
        """Analyze Langflow workflow structure without conversion.
        
        Args:
            langflow_data: Parsed Langflow JSON data
            
        Returns:
            Analysis results
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
            analysis["dependencies"] = self.dependency_analyzer.build_dependency_graph(edges)
            
            return analysis
            
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