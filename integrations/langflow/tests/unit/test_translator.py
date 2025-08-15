"""Unit tests for LangflowConverter."""

import pytest
from typing import Dict, Any

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.utils.errors import ConversionError


class TestLangflowConverter:
    """Test LangflowConverter functionality."""
    
    def test_init_default(self):
        """Test default initialization."""
        converter = LangflowConverter()
        assert not converter.validate_schemas
    
    def test_init_with_validation(self):
        """Test initialization with validation enabled."""
        converter = LangflowConverter(validate_schemas=True)
        assert converter.validate_schemas
    
    def test_convert_simple_workflow(
        self, 
        converter: LangflowConverter, 
        simple_langflow_workflow: Dict[str, Any]
    ):
        """Test conversion of simple workflow."""
        workflow = converter.convert(simple_langflow_workflow)
        
        assert workflow.name == "Converted Langflow Workflow"
        assert len(workflow.steps) == 2
        
        # Check step IDs
        step_ids = [step.id for step in workflow.steps]
        assert "langflow_chatinput" in step_ids
        assert "langflow_chatoutput" in step_ids
        
        # Check components
        for step in workflow.steps:
            assert step.component == "langflow://udf_executor"
            assert "blob_id" in step.input
    
    def test_convert_empty_nodes(self, converter: LangflowConverter):
        """Test conversion with empty nodes list."""
        workflow_data = {"data": {"nodes": [], "edges": []}}
        
        with pytest.raises(ConversionError, match="No nodes found"):
            converter.convert(workflow_data)
    
    def test_convert_missing_data(self, converter: LangflowConverter):
        """Test conversion with missing data key."""
        workflow_data = {"invalid": "structure"}
        
        with pytest.raises(ConversionError, match="missing 'data' key"):
            converter.convert(workflow_data)
    
    def test_to_yaml(
        self, 
        converter: LangflowConverter, 
        simple_langflow_workflow: Dict[str, Any]
    ):
        """Test YAML generation."""
        workflow = converter.convert(simple_langflow_workflow)
        yaml_output = converter.to_yaml(workflow)
        
        assert isinstance(yaml_output, str)
        assert "name: Converted Langflow Workflow" in yaml_output
        assert "steps:" in yaml_output
        assert "langflow://udf_executor" in yaml_output
    
    def test_analyze_workflow(
        self, 
        converter: LangflowConverter, 
        simple_langflow_workflow: Dict[str, Any]
    ):
        """Test workflow analysis."""
        analysis = converter.analyze(simple_langflow_workflow)
        
        assert analysis["node_count"] == 2
        assert analysis["edge_count"] == 1
        assert "ChatInput" in analysis["component_types"]
        assert "ChatOutput" in analysis["component_types"]
        assert analysis["component_types"]["ChatInput"] == 1
        assert analysis["component_types"]["ChatOutput"] == 1