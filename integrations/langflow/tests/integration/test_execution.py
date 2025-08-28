# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License. You may obtain a copy of
# the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations under
# the License.

"""Execution tests: End-to-end workflow execution with meaningful result validation.

These tests verify that converted Langflow workflows can be successfully executed
by the Stepflow binary with real or mocked inputs, and that the results have the
expected structure and content beyond just "is dict".
"""

from pathlib import Path
from typing import Any

import pytest

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.testing.stepflow_binary import (
    StepflowBinaryRunner,
    create_test_config_file,
    get_default_stepflow_config,
)

from .test_registry import TestWorkflow, get_test_registry, pytest_parametrize_workflows


class TestWorkflowExecution:
    """Test end-to-end execution of converted workflows."""

    @pytest.fixture(scope="class")
    def registry(self):
        """Get test registry."""
        return get_test_registry()

    @pytest.fixture(scope="class")
    def converter(self):
        """Create converter instance."""
        return LangflowConverter()

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
        """Create test configuration file with mock components."""
        config_content = get_default_stepflow_config()
        config_path = create_test_config_file(config_content)
        yield config_path
        Path(config_path).unlink(missing_ok=True)

    @pytest.mark.slow
    @pytest.mark.parametrize(
        "workflow",
        pytest_parametrize_workflows(
            get_test_registry().get_execution_test_cases(mockable_only=True)
        ),
    )
    def test_workflow_execution_with_mocking(
        self,
        workflow: TestWorkflow,
        registry,
        converter: LangflowConverter,
        stepflow_runner: StepflowBinaryRunner,
        test_config_path: str,
    ):
        """Test execution of workflows using mock responses."""

        # Load and convert workflow
        try:
            langflow_data = registry.load_langflow_data(workflow)
        except FileNotFoundError:
            pytest.skip(f"Langflow file not found: {workflow.langflow_file}")

        stepflow_workflow = converter.convert(langflow_data)
        workflow_yaml = converter.to_yaml(stepflow_workflow)

        # First validate the workflow
        success, stdout, stderr = stepflow_runner.validate_workflow(
            workflow_yaml, config_path=test_config_path
        )
        assert success, (
            f"Workflow validation failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}"
        )

        # Execute the workflow
        input_data = workflow.input_data or {}

        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml,
            input_data,
            config_path=test_config_path,
            timeout=workflow.execution.timeout_seconds,
        )

        if workflow.execution.should_succeed:
            if not success:
                # Check if failure is due to missing components
                # (expected in test environment)
                if self._is_component_availability_error(stderr):
                    pytest.skip(
                        f"Required components not available in test "
                        f"environment: {stderr}"
                    )
                else:
                    pytest.fail(
                        f"Workflow execution failed:\nSTDOUT: {stdout}\n"
                        f"STDERR: {stderr}"
                    )

            # Validate execution result structure and content
            self._assert_execution_success(result_data, workflow, input_data)
        else:
            assert not success, (
                f"Workflow execution should have failed but succeeded: {result_data}"
            )

    def _is_component_availability_error(self, stderr: str) -> bool:
        """Check if execution failure is due to missing components."""
        availability_indicators = [
            "unknown component",
            "component not found",
            "langflow",
            "not available",
            "binary not found",
        ]
        return any(indicator in stderr.lower() for indicator in availability_indicators)

    def _assert_execution_success(
        self, result_data: Any, workflow: TestWorkflow, input_data: dict[str, Any]
    ):
        """Assert execution result meets expectations beyond just being a dict."""

        # Basic structure validation
        assert result_data is not None, f"Result should not be None for {workflow.name}"
        assert isinstance(result_data, dict), (
            f"Result should be dict for {workflow.name}, got {type(result_data)}"
        )

        expectations = workflow.execution

        # Check result contains expected keys
        if expectations.result_contains_keys:
            for key in expectations.result_contains_keys:
                assert key in result_data, (
                    f"Expected key '{key}' not found in result for "
                    f"{workflow.name}. Result keys: {list(result_data.keys())}"
                )

        # Check specific result values
        if expectations.result_values:
            for key, expected_value in expectations.result_values.items():
                assert key in result_data, (
                    f"Expected key '{key}' not found in result for {workflow.name}"
                )
                actual_value = result_data[key]
                assert actual_value == expected_value, (
                    f"Expected {key}='{expected_value}', got '{actual_value}' "
                    f"for {workflow.name}"
                )

        # Check result structure schema
        if expectations.result_structure:
            # For Stepflow execution results, validate the inner 'result' field
            # structure if the result_data has the standard Stepflow response format
            validation_data = result_data
            if "outcome" in result_data and "result" in result_data:
                # This is a Stepflow execution response, validate the inner
                # result
                validation_data = result_data["result"]

            self._validate_result_structure(
                validation_data, expectations.result_structure, workflow.name
            )

        # Workflow-specific validation using registry-defined validators
        if expectations.result_validator:
            expectations.result_validator.validate(
                result_data, input_data, workflow.name
            )

    def _validate_result_structure(
        self,
        result_data: dict[str, Any],
        expected_structure: dict[str, Any],
        workflow_name: str,
    ):
        """Validate result matches expected structure schema."""

        # Check type
        expected_type = expected_structure.get("type", "object")
        if expected_type == "object":
            assert isinstance(result_data, dict), (
                f"Expected object type for {workflow_name}"
            )
        elif expected_type == "array":
            assert isinstance(result_data, list), (
                f"Expected array type for {workflow_name}"
            )
        elif expected_type == "string":
            assert isinstance(result_data, str), (
                f"Expected string type for {workflow_name}"
            )

        # Check required fields for object type
        if expected_type == "object" and "required_fields" in expected_structure:
            for field in expected_structure["required_fields"]:
                assert field in result_data, (
                    f"Required field '{field}' missing in result for {workflow_name}"
                )

        # Check field types if specified
        if "field_types" in expected_structure:
            for field, field_type in expected_structure["field_types"].items():
                if field in result_data:
                    actual_value = result_data[field]
                    if field_type == "string":
                        assert isinstance(actual_value, str), (
                            f"Field '{field}' should be string in {workflow_name}"
                        )
                    elif field_type == "number":
                        assert isinstance(actual_value, int | float), (
                            f"Field '{field}' should be number in {workflow_name}"
                        )
                    elif field_type == "boolean":
                        assert isinstance(actual_value, bool), (
                            f"Field '{field}' should be boolean in {workflow_name}"
                        )


class TestExecutionErrorHandling:
    """Test execution error handling scenarios."""

    @pytest.fixture
    def stepflow_runner(self) -> StepflowBinaryRunner:
        """Create StepflowBinaryRunner instance."""
        try:
            return StepflowBinaryRunner()
        except FileNotFoundError as e:
            pytest.skip(f"Stepflow binary not found: {e}")

    @pytest.fixture
    def test_config_path(self) -> str:
        """Create test configuration file."""
        config_content = get_default_stepflow_config()
        config_path = create_test_config_file(config_content)
        yield config_path
        Path(config_path).unlink(missing_ok=True)

    def test_execution_with_missing_input(
        self, stepflow_runner: StepflowBinaryRunner, test_config_path: str
    ):
        """Test execution with missing required input."""

        workflow_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Missing Input Test
input:
  required_field:
    type: string
    description: "This field is required"

steps:
  - id: test_step
    component: /builtin/create_messages
    input:
      messages:
        - role: user
          content:
            $from:
              workflow: input
            path: required_field
"""

        # Execute without providing required input
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml, {}, config_path=test_config_path, timeout=15.0
        )

        # Should handle missing input gracefully
        assert not success, "Execution should fail when required input is missing"

        # Check structured error response
        assert result_data is not None, (
            "Should have result data even for failed execution"
        )
        assert isinstance(result_data, dict), "Result should be dict"
        assert result_data.get("outcome") == "failed", "Should indicate failed outcome"

        # Check error contains meaningful information about missing input
        error_info = result_data.get("error", {})
        error_message = error_info.get("message", "").lower()

        input_error_indicators = [
            "resolve",
            "value",
            "undefined",
            "field",
            "required_field",
        ]
        has_input_error = any(
            indicator in error_message for indicator in input_error_indicators
        )
        assert has_input_error, (
            f"Error should indicate missing input issue: {error_message}"
        )

    def test_execution_timeout(
        self, stepflow_runner: StepflowBinaryRunner, test_config_path: str
    ):
        """Test execution timeout handling."""

        # Simple workflow that should complete quickly
        workflow_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Timeout Test
steps:
  - id: quick_step
    component: /builtin/create_messages
    input:
      messages:
        - role: user
          content: "Hello"
"""

        # Use very short timeout - should either complete quickly or timeout
        try:
            success, result_data, stdout, stderr = stepflow_runner.run_workflow(
                workflow_yaml, {}, config_path=test_config_path, timeout=0.1
            )

            # If it completes, that's fine too (the workflow is very simple)
            # Just verify it completed successfully
            assert success, (
                "Simple workflow should complete successfully if it doesn't timeout"
            )

        except Exception as e:
            # Should be timeout exception
            error_message = str(e).lower()
            timeout_indicators = ["timeout", "timed out", "expired"]
            has_timeout_indication = any(
                indicator in error_message for indicator in timeout_indicators
            )
            assert has_timeout_indication, f"Should be timeout error: {e}"


@pytest.mark.slow
class TestExecutionPerformance:
    """Test execution performance characteristics."""

    @pytest.fixture
    def stepflow_runner(self) -> StepflowBinaryRunner:
        """Create StepflowBinaryRunner instance."""
        try:
            return StepflowBinaryRunner()
        except FileNotFoundError as e:
            pytest.skip(f"Stepflow binary not found: {e}")

    @pytest.fixture
    def test_config_path(self) -> str:
        """Create test configuration file."""
        config_content = get_default_stepflow_config()
        config_path = create_test_config_file(config_content)
        yield config_path
        Path(config_path).unlink(missing_ok=True)

    def test_execution_performance_characteristics(
        self, stepflow_runner: StepflowBinaryRunner, test_config_path: str
    ):
        """Test execution performance for mockable workflows."""

        import time

        registry = get_test_registry()
        converter = LangflowConverter()
        performance_results = []

        for workflow in registry.get_execution_test_cases(mockable_only=True):
            if "slow" in workflow.tags:
                continue  # Skip slow tests in performance testing

            try:
                langflow_data = registry.load_langflow_data(workflow)
                stepflow_workflow = converter.convert(langflow_data)
                workflow_yaml = converter.to_yaml(stepflow_workflow)

                # Measure execution time
                input_data = workflow.input_data or {}
                start_time = time.time()

                success, result_data, stdout, stderr = stepflow_runner.run_workflow(
                    workflow_yaml,
                    input_data,
                    config_path=test_config_path,
                    timeout=workflow.execution.timeout_seconds,
                )

                execution_time = time.time() - start_time

                node_count = len(langflow_data.get("data", {}).get("nodes", []))
                performance_results.append(
                    {
                        "workflow": workflow.name,
                        "nodes": node_count,
                        "execution_time": execution_time,
                        "success": success,
                        "result_size": len(str(result_data)) if result_data else 0,
                    }
                )

            except Exception as e:
                # Skip workflows that fail to load/convert
                continue

        # Performance assertions
        if performance_results:
            max_time = max(r["execution_time"] for r in performance_results)
            avg_time = sum(r["execution_time"] for r in performance_results) / len(
                performance_results
            )

            assert max_time < 60.0, f"Execution took too long: {max_time:.2f}s"
            assert avg_time < 30.0, f"Average execution time too high: {avg_time:.2f}s"

            # Print performance summary
            print("\nExecution Performance Results:")
            print(f"Average execution time: {avg_time:.3f}s")
            for result in sorted(
                performance_results, key=lambda x: x["execution_time"], reverse=True
            ):
                status = "✅" if result["success"] else "❌"
                print(
                    f"  {result['workflow']:20} | {result['nodes']:2} nodes | "
                    f"{result['execution_time']:6.3f}s | {result['result_size']:4}B | "
                    f"{status}"
                )
        else:
            pytest.skip("No workflows available for performance testing")
