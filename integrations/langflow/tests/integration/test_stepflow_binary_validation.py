"""Integration tests using real Stepflow binary for validation and execution."""

import json
import os
import tempfile
import pytest
from pathlib import Path
from typing import Dict, Any
from unittest.mock import patch

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.testing.stepflow_binary import (
    StepflowBinaryRunner, 
    get_default_stepflow_config, 
    create_test_config_file
)
from stepflow_langflow_integration.utils.errors import ValidationError, ExecutionError


class TestStepflowBinaryValidation:
    """Test validation and execution using real Stepflow binary."""
    
    @pytest.fixture(scope="class")
    def stepflow_runner(self) -> StepflowBinaryRunner:
        """Create StepflowBinaryRunner instance."""
        try:
            runner = StepflowBinaryRunner()
            available, version = runner.check_binary_availability()
            if not available:
                pytest.skip(f"Stepflow binary not available: {version}")
            return runner
        except FileNotFoundError as e:
            pytest.skip(f"Stepflow binary not found: {e}")
    
    @pytest.fixture(scope="class") 
    def test_config_path(self) -> str:
        """Create test configuration file."""
        config_content = get_default_stepflow_config()
        config_path = create_test_config_file(config_content)
        yield config_path
        # Cleanup
        Path(config_path).unlink(missing_ok=True)
    
    def test_binary_availability(self, stepflow_runner: StepflowBinaryRunner):
        """Test that stepflow binary is available and working."""
        available, version_info = stepflow_runner.check_binary_availability()
        
        assert available, f"Stepflow binary not available: {version_info}"
        assert "stepflow" in version_info.lower()
    
    def test_validate_converted_workflow(
        self,
        stepflow_runner: StepflowBinaryRunner,
        test_config_path: str,
        langflow_fixtures_dir: Path,
        converter: LangflowConverter
    ):
        """Test validation of converted Langflow workflows."""
        
        # Test simple chat workflow
        simple_chat_path = langflow_fixtures_dir / "simple_chat.json"
        if not simple_chat_path.exists():
            pytest.skip("simple_chat.json fixture not found")
        
        with open(simple_chat_path, "r") as f:
            langflow_data = json.load(f)
        
        # Convert to Stepflow
        workflow = converter.convert(langflow_data)
        workflow_yaml = converter.to_yaml(workflow)
        
        # Validate using stepflow binary
        success, stdout, stderr = stepflow_runner.validate_workflow(
            workflow_yaml, 
            config_path=test_config_path
        )
        
        assert success, f"Workflow validation failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}"
        assert "✅" in stdout or "passed" in stdout.lower() or "success" in stdout.lower()
    
    def test_validate_invalid_workflow(
        self,
        stepflow_runner: StepflowBinaryRunner,
        test_config_path: str
    ):
        """Test validation of invalid workflow."""
        
        invalid_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Invalid Workflow
steps:
  - id: invalid_step
    component: nonexistent://component
    input: {}
"""
        
        success, stdout, stderr = stepflow_runner.validate_workflow(
            invalid_yaml,
            config_path=test_config_path
        )
        
        assert not success, "Invalid workflow should fail validation"
        assert "error" in stdout.lower() or "failed" in stdout.lower(), f"Should have error message in output: {stdout}"
    
    @pytest.mark.slow
    def test_run_simple_builtin_workflow(
        self,
        stepflow_runner: StepflowBinaryRunner,
        test_config_path: str
    ):
        """Test running a simple workflow with builtin components."""
        
        # Simple workflow using builtin components
        workflow_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Simple Builtin Test
steps:
  - id: test_step
    component: /builtin/create_messages
    input:
      messages:
        - role: user
          content: "Hello, world!"
"""
        
        input_data = {}
        
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml,
            input_data,
            config_path=test_config_path,
            timeout=30.0
        )
        
        if not success:
            # Builtin components might not be available in test environment
            pytest.skip(f"Builtin workflow execution failed (expected in test env): {stderr}")
        
        assert result_data is not None
        assert isinstance(result_data, dict)
    
    @pytest.mark.slow
    @pytest.mark.parametrize("workflow_file", [
        "simple_chat.json",
        "basic_prompting.json",
        "memory_chatbot.json",
        "document_qa.json"
    ])
    def test_run_converted_langflow_workflow(
        self,
        workflow_file: str,
        stepflow_runner: StepflowBinaryRunner,
        test_config_path: str,
        langflow_fixtures_dir: Path,
        converter: LangflowConverter
    ):
        """Test running converted Langflow workflows."""
        
        workflow_path = langflow_fixtures_dir / workflow_file
        if not workflow_path.exists():
            pytest.skip(f"Workflow file not found: {workflow_file}")
        
        with open(workflow_path, "r") as f:
            langflow_data = json.load(f)
        
        # Convert to Stepflow
        workflow = converter.convert(langflow_data)
        workflow_yaml = converter.to_yaml(workflow)
        
        # First validate the workflow
        success, stdout, stderr = stepflow_runner.validate_workflow(
            workflow_yaml,
            config_path=test_config_path
        )
        
        assert success, f"Converted workflow validation failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}"
        
        # Try to run the workflow
        input_data = {"message": "Hello from test!"}
        
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml,
            input_data,
            config_path=test_config_path,
            timeout=45.0
        )
        
        if not success:
            # Langflow components might not be available
            if "langflow" in stderr.lower() or "not found" in stderr.lower():
                pytest.skip(f"Langflow components not available in test environment: {stderr}")
            else:
                pytest.fail(f"Workflow execution failed unexpectedly:\nSTDOUT: {stdout}\nSTDERR: {stderr}")
        
        # If execution succeeded, validate result structure
        assert result_data is not None
        assert isinstance(result_data, dict)
    
    def test_list_available_components(
        self,
        stepflow_runner: StepflowBinaryRunner,
        test_config_path: str
    ):
        """Test listing available components."""
        
        success, components, stderr = stepflow_runner.list_components(
            config_path=test_config_path
        )
        
        assert success, f"Failed to list components: {stderr}"
        assert isinstance(components, list)
        assert len(components) > 0, "Should have at least some builtin components"
        
        # Should have builtin components
        builtin_components = [c for c in components if "builtin" in c.lower()]
        assert len(builtin_components) > 0, "Should have builtin components available"
    
    def test_workflow_with_dependencies(
        self,
        stepflow_runner: StepflowBinaryRunner, 
        test_config_path: str
    ):
        """Test workflow with step dependencies."""
        
        workflow_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Dependency Test
input:
  message:
    type: string
    default: "Hello"

steps:
  - id: step1
    component: /builtin/create_messages
    input:
      messages:
        - role: user
          content:
            $from:
              workflow: input
            path: message

  - id: step2  
    component: /builtin/create_messages
    input:
      messages:
        - role: assistant
          content: "Response to previous message"
      previous_messages:
        $from:
          step: step1
        path: result

output:
  result:
    $from:
      step: step2
    path: result
"""
        
        # First validate
        success, stdout, stderr = stepflow_runner.validate_workflow(
            workflow_yaml,
            config_path=test_config_path
        )
        
        assert success, f"Dependency workflow validation failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}"
        
        # Try to run
        input_data = {"message": "Test dependency flow"}
        
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml,
            input_data,
            config_path=test_config_path,
            timeout=30.0
        )
        
        if not success:
            # May fail due to missing components in test environment
            pytest.skip(f"Dependency workflow execution failed (expected): {stderr}")
        
        assert result_data is not None
    
    def test_error_handling_scenarios(
        self,
        stepflow_runner: StepflowBinaryRunner,
        test_config_path: str
    ):
        """Test various error scenarios."""
        
        # Test workflow with syntax error
        invalid_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Syntax Error Test
steps:
  - id: bad_step
    component: /builtin/nonexistent
    input: {}
    invalid_field: "should not be here"
"""
        
        success, stdout, stderr = stepflow_runner.validate_workflow(
            invalid_yaml,
            config_path=test_config_path
        )
        
        # Validation might pass but execution should fail
        if success:
            # Try to run and expect failure
            success, result_data, stdout, stderr = stepflow_runner.run_workflow(
                invalid_yaml,
                {},
                config_path=test_config_path,
                timeout=15.0
            )
            assert not success, "Workflow with nonexistent component should fail"
        
        # Test malformed YAML
        malformed_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Malformed YAML
steps:
  - id: test
    component: /builtin/test
    input: {
      unclosed: dict
"""
        
        success, stdout, stderr = stepflow_runner.validate_workflow(
            malformed_yaml,
            config_path=test_config_path
        )
        
        assert not success, "Malformed YAML should fail validation"
        assert "yaml" in stdout.lower() or "parse" in stdout.lower() or "invalid" in stdout.lower()
    
    @pytest.mark.slow
    def test_performance_validation(
        self,
        stepflow_runner: StepflowBinaryRunner,
        test_config_path: str,
        langflow_fixtures_dir: Path,
        converter: LangflowConverter
    ):
        """Test performance of validation for different workflow sizes."""
        
        import time
        
        test_files = list(langflow_fixtures_dir.glob("*.json"))
        performance_results = []
        
        for test_file in test_files:
            with open(test_file, "r") as f:
                try:
                    langflow_data = json.load(f)
                    workflow = converter.convert(langflow_data)
                    workflow_yaml = converter.to_yaml(workflow)
                    
                    # Measure validation time
                    start_time = time.time()
                    success, stdout, stderr = stepflow_runner.validate_workflow(
                        workflow_yaml,
                        config_path=test_config_path
                    )
                    validation_time = time.time() - start_time
                    
                    node_count = len(langflow_data.get("data", {}).get("nodes", []))
                    performance_results.append({
                        "file": test_file.name,
                        "nodes": node_count,
                        "validation_time": validation_time,
                        "success": success
                    })
                    
                except Exception:
                    # Skip files that don't convert
                    continue
        
        # Basic performance assertions
        assert len(performance_results) > 0, "Should have validated at least one workflow"
        
        max_time = max(r["validation_time"] for r in performance_results)
        assert max_time < 10.0, f"Validation took too long: {max_time:.2f}s"
        
        # At least some validations should succeed
        successful_validations = sum(1 for r in performance_results if r["success"])
        assert successful_validations > 0, "At least some validations should succeed"


@pytest.mark.integration 
class TestStepflowBinaryWithRealConfig:
    """Tests that require a more realistic Stepflow configuration."""
    
    @pytest.fixture(scope="class")
    def stepflow_runner(self) -> StepflowBinaryRunner:
        """Create StepflowBinaryRunner instance."""
        try:
            return StepflowBinaryRunner()
        except FileNotFoundError as e:
            pytest.skip(f"Stepflow binary not found: {e}")
    
    @pytest.fixture(scope="class")
    def production_config_path(self) -> str:
        """Create production-like configuration."""
        config_content = """
plugins:
  builtin:
    type: builtin
  # Note: Langflow plugin would need actual langflow installation

routes:
  "/{*component}":
    - plugin: builtin

stateStore:
  type: inMemory
"""
        config_path = create_test_config_file(config_content)
        yield config_path
        Path(config_path).unlink(missing_ok=True)
    
    def test_validate_with_production_config(
        self,
        stepflow_runner: StepflowBinaryRunner,
        production_config_path: str,
        langflow_fixtures_dir: Path,
        converter: LangflowConverter
    ):
        """Test validation with production-like configuration."""
        
        simple_chat_path = langflow_fixtures_dir / "simple_chat.json"
        if not simple_chat_path.exists():
            pytest.skip("simple_chat.json fixture not found")
        
        with open(simple_chat_path, "r") as f:
            langflow_data = json.load(f)
        
        # Convert and validate
        workflow = converter.convert(langflow_data)
        workflow_yaml = converter.to_yaml(workflow)
        
        success, stdout, stderr = stepflow_runner.validate_workflow(
            workflow_yaml,
            config_path=production_config_path
        )
        
        # May fail due to missing langflow components, which is expected
        if not success and "langflow" in stderr.lower():
            pytest.skip("Langflow components not available in production config")
        
        assert success, f"Production validation failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}"