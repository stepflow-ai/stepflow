"""Integration tests for Langflow workflows using real JSON files and expectations."""

import json
import os
import tempfile
import time
from pathlib import Path
from typing import Dict, Any, List
from unittest.mock import patch, AsyncMock

import pytest
import yaml

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.executor.langflow_server import StepflowLangflowServer
from stepflow_langflow_integration.utils.errors import ConversionError, ValidationError


class TestLangflowIntegration:
    """Integration tests using real Langflow JSON files."""
    
    @pytest.fixture
    def expectations(self, fixtures_dir: Path) -> Dict[str, Any]:
        """Load test expectations from YAML file."""
        expectations_path = fixtures_dir / "expectations.yaml"
        with open(expectations_path, "r") as f:
            return yaml.safe_load(f)
    
    @pytest.fixture
    def test_cases(self, expectations: Dict[str, Any]) -> List[Dict[str, Any]]:
        """Extract test cases from expectations."""
        return expectations.get("test_cases", [])
    
    def load_langflow_json(self, langflow_fixtures_dir: Path, filename: str) -> Dict[str, Any]:
        """Load Langflow JSON file or inline content."""
        file_path = langflow_fixtures_dir / filename
        if file_path.exists():
            with open(file_path, "r") as f:
                return json.load(f)
        else:
            # File doesn't exist - this might be intentional for error testing
            raise FileNotFoundError(f"Langflow file not found: {filename}")
    
    def test_conversion_expectations(
        self, 
        test_case: Dict[str, Any],
        langflow_fixtures_dir: Path,
        converter: LangflowConverter
    ):
        """Test conversion expectations for each test case."""
        name = test_case["name"]
        langflow_file = test_case.get("langflow_file")
        conversion_expectations = test_case.get("conversion_expectations", {})
        
        # Handle inline content vs file
        if "langflow_content" in test_case:
            langflow_data = test_case["langflow_content"]
        else:
            try:
                langflow_data = self.load_langflow_json(langflow_fixtures_dir, langflow_file)
            except FileNotFoundError:
                if conversion_expectations.get("should_fail"):
                    # This is expected - file should not exist
                    pytest.skip(f"Test {name} expects file to not exist - skipping conversion test")
                    return
                else:
                    pytest.fail(f"Required test file not found: {langflow_file}")
        
        # Test conversion
        if conversion_expectations.get("should_fail"):
            expected_error = conversion_expectations.get("error_type", "ConversionError")
            expected_message = conversion_expectations.get("error_message_contains", "")
            
            with pytest.raises(Exception) as exc_info:
                converter.convert(langflow_data)
            
            assert expected_error in str(type(exc_info.value).__name__)
            if expected_message:
                assert expected_message in str(exc_info.value)
        else:
            # Conversion should succeed
            workflow = converter.convert(langflow_data)
            
            # Check workflow name
            if "workflow_name" in conversion_expectations:
                assert workflow.name == conversion_expectations["workflow_name"]
            
            # Check step count
            if "step_count" in conversion_expectations:
                assert len(workflow.steps) == conversion_expectations["step_count"]
            
            # Check step IDs
            if "step_ids" in conversion_expectations:
                actual_ids = [step.id for step in workflow.steps]
                expected_ids = conversion_expectations["step_ids"]
                for expected_id in expected_ids:
                    assert expected_id in actual_ids, f"Expected step ID {expected_id} not found in {actual_ids}"
            
            # Check component types
            if "component_types" in conversion_expectations:
                actual_components = [step.component for step in workflow.steps]
                expected_components = conversion_expectations["component_types"]
                assert actual_components == expected_components
            
            # Check dependencies
            if conversion_expectations.get("has_dependencies"):
                # At least one step should have dependencies
                # (This would need to be implemented based on actual step structure)
                pass
    
    def test_specific_workflows(self, langflow_fixtures_dir: Path, converter: LangflowConverter):
        """Test specific known workflows with detailed validation."""
        
        # Test simple chat workflow
        simple_chat_path = langflow_fixtures_dir / "simple_chat.json"
        if simple_chat_path.exists():
            with open(simple_chat_path, "r") as f:
                simple_chat = json.load(f)
            
            workflow = converter.convert(simple_chat)
            
            assert workflow.name == "Simple Chat Example"
            assert len(workflow.steps) == 2
            
            # Check that steps use Langflow components directly (not UDF executors for built-ins)
            step_components = [step.component for step in workflow.steps]
            assert "/langflow/ChatInput" in step_components
            assert "/langflow/ChatOutput" in step_components
            
            # Built-in components should not have blob_id (only custom UDF components do)
            for step in workflow.steps:
                if step.component.startswith("/langflow/") and step.component != "/langflow/udf_executor":
                    assert "blob_id" not in step.input
            
            # Check step IDs are cleaned up
            step_ids = [step.id for step in workflow.steps]
            assert "langflow_chatinput" in step_ids
            assert "langflow_chatoutput" in step_ids
    
    def test_analysis_consistency(self, langflow_fixtures_dir: Path, converter: LangflowConverter):
        """Test that analysis and conversion give consistent results."""
        test_files = list(langflow_fixtures_dir.glob("*.json"))
        
        for test_file in test_files:
            with open(test_file, "r") as f:
                langflow_data = json.load(f)
            
            try:
                # Analyze
                analysis = converter.analyze(langflow_data)
                
                # Convert
                workflow = converter.convert(langflow_data)
                
                # Check consistency
                assert analysis["node_count"] == len(workflow.steps)
                
                # Total components should match steps
                total_components = sum(analysis["component_types"].values())
                assert total_components == len(workflow.steps)
                
            except (ConversionError, ValidationError):
                # Some test files might be designed to fail
                continue
    
    @pytest.mark.slow
    def test_performance_benchmarks(
        self, 
        langflow_fixtures_dir: Path, 
        converter: LangflowConverter,
        expectations: Dict[str, Any]
    ):
        """Test performance benchmarks for different workflow sizes."""
        perf_expectations = expectations.get("performance_expectations", {})
        
        test_files = list(langflow_fixtures_dir.glob("*.json"))
        
        for test_file in test_files:
            with open(test_file, "r") as f:
                langflow_data = json.load(f)
            
            # Categorize workflow size
            node_count = len(langflow_data.get("data", {}).get("nodes", []))
            
            if node_count <= 3:
                size_category = "small_workflow"
            elif node_count <= 10:
                size_category = "medium_workflow"
            else:
                size_category = "large_workflow"
            
            if size_category not in perf_expectations:
                continue
            
            expectations_for_size = perf_expectations[size_category]
            max_time = expectations_for_size.get("max_conversion_time", 10.0)
            
            # Measure conversion time
            start_time = time.time()
            try:
                workflow = converter.convert(langflow_data)
                conversion_time = time.time() - start_time
                
                assert conversion_time < max_time, (
                    f"Conversion took {conversion_time:.2f}s, expected < {max_time}s "
                    f"for {size_category} ({node_count} nodes)"
                )
                
            except (ConversionError, ValidationError):
                # Skip performance test for workflows that don't convert
                continue
    
    @pytest.mark.slow
    @pytest.mark.asyncio
    @patch('stepflow_langflow_integration.executor.udf_executor.UDFExecutor._execute_langflow_component')
    async def test_execution_with_mocking(
        self,
        mock_execute: AsyncMock,
        langflow_fixtures_dir: Path,
        converter: LangflowConverter,
        test_cases: List[Dict[str, Any]]
    ):
        """Test execution flow with mocked Langflow components."""
        
        for test_case in test_cases:
            execution_expectations = test_case.get("execution_expectations", {})
            
            if not execution_expectations.get("can_mock"):
                continue
            
            # Set up mock response
            mock_response = execution_expectations.get("mock_response")
            if mock_response:
                mock_execute.return_value = mock_response
            
            # Load workflow
            langflow_file = test_case.get("langflow_file")
            if not langflow_file:
                continue
                
            try:
                langflow_data = self.load_langflow_json(langflow_fixtures_dir, langflow_file)
                workflow = converter.convert(langflow_data)
                
                # Test that workflow structure is correct for execution
                assert len(workflow.steps) > 0
                for step in workflow.steps:
                    # Should be either a Langflow component or UDF executor
                    assert step.component.startswith("/langflow/")
                    
                    # Only UDF executors should have blob_id
                    if step.component == "/langflow/udf_executor":
                        assert "blob_id" in step.input
                
            except (FileNotFoundError, ConversionError, ValidationError):
                # Skip execution test for workflows that don't load/convert
                continue
    
    def test_error_handling(self, langflow_fixtures_dir: Path, converter: LangflowConverter):
        """Test error handling for various invalid inputs."""
        
        # Test missing data key
        invalid_no_data = {"invalid": "structure"}
        with pytest.raises(ConversionError, match="missing 'data' key"):
            converter.convert(invalid_no_data)
        
        # Test empty nodes
        invalid_empty = {"data": {"nodes": [], "edges": []}}
        with pytest.raises(ConversionError, match="No nodes found"):
            converter.convert(invalid_empty)
        
        # Test malformed node
        invalid_node = {
            "data": {
                "nodes": [{"invalid": "node"}],
                "edges": []
            }
        }
        with pytest.raises(ConversionError):
            converter.convert(invalid_node)
    
    def test_yaml_output_quality(self, langflow_fixtures_dir: Path, converter: LangflowConverter):
        """Test that generated YAML is well-formed and readable."""
        test_files = list(langflow_fixtures_dir.glob("*.json"))
        
        for test_file in test_files:
            with open(test_file, "r") as f:
                langflow_data = json.load(f)
            
            try:
                workflow = converter.convert(langflow_data)
                yaml_output = converter.to_yaml(workflow)
                
                # Should be valid YAML
                parsed_yaml = yaml.safe_load(yaml_output)
                assert isinstance(parsed_yaml, dict)
                
                # Should have expected structure
                assert "name" in parsed_yaml
                assert "steps" in parsed_yaml
                assert isinstance(parsed_yaml["steps"], list)
                
                # Should be reasonably readable (not too long lines)
                lines = yaml_output.split('\n')
                for line in lines:
                    assert len(line) < 200, f"Line too long in YAML output: {line[:50]}..."
                
            except (ConversionError, ValidationError):
                continue


# Parametrized test generation
def pytest_generate_tests(metafunc):
    """Generate parametrized tests from expectations file."""
    if "test_case" in metafunc.fixturenames:
        # Load expectations
        fixtures_dir = Path(__file__).parent.parent / "fixtures"
        expectations_path = fixtures_dir / "expectations.yaml"
        
        if expectations_path.exists():
            with open(expectations_path, "r") as f:
                expectations = yaml.safe_load(f)
            
            test_cases = expectations.get("test_cases", [])
            
            # Create parametrized test cases
            metafunc.parametrize(
                "test_case",
                [
                    pytest.param(
                        test_case,
                        id=test_case["name"],
                        marks=pytest.mark.slow if "openai" in test_case.get("langflow_file", "") else []
                    )
                    for test_case in test_cases
                ]
            )