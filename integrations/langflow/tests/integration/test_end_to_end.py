"""End-to-end integration tests."""

import json
import tempfile
from pathlib import Path
from typing import Dict, Any

import pytest
import yaml

from stepflow_langflow_integration.converter.translator import LangflowConverter


class TestEndToEnd:
    """Test end-to-end conversion workflow."""
    
    def test_convert_file_simple_workflow(self, simple_langflow_workflow: Dict[str, Any]):
        """Test converting a simple workflow from file."""
        converter = LangflowConverter()
        
        # Create temporary input file
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump(simple_langflow_workflow, f)
            input_path = Path(f.name)
        
        try:
            # Convert
            yaml_output = converter.convert_file(input_path)
            
            # Parse YAML to verify structure
            workflow_dict = yaml.safe_load(yaml_output)
            
            assert "name" in workflow_dict
            assert "steps" in workflow_dict
            assert len(workflow_dict["steps"]) == 2
            
            # Check step structure
            for step in workflow_dict["steps"]:
                assert "id" in step
                assert "component" in step
                assert "input" in step
                assert step["component"].startswith("/langflow/")
                
        finally:
            input_path.unlink()  # Clean up temp file
    
    @pytest.mark.slow
    def test_convert_with_validation(self, simple_langflow_workflow: Dict[str, Any]):
        """Test conversion with validation enabled."""
        converter = LangflowConverter(validate_schemas=True)
        
        # This should not raise an exception
        workflow = converter.convert(simple_langflow_workflow)
        assert workflow is not None
    
    def test_analyze_and_convert_consistency(self, simple_langflow_workflow: Dict[str, Any]):
        """Test that analyze and convert give consistent results."""
        converter = LangflowConverter()
        
        # Analyze
        analysis = converter.analyze(simple_langflow_workflow)
        
        # Convert
        workflow = converter.convert(simple_langflow_workflow)
        
        # Check consistency
        assert analysis["node_count"] == len(workflow.steps)
        
        # Component types should match step components (all UDF executors)
        total_components = sum(analysis["component_types"].values())
        assert total_components == len(workflow.steps)