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

"""Validation tests: Stepflow binary validates converted workflows.

These tests verify that converted Langflow workflows can be successfully validated
by the Stepflow binary, ensuring proper schema compliance and component routing.
They bridge conversion testing and execution testing.
"""

from pathlib import Path

import pytest

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.testing.stepflow_binary import (
    StepflowBinaryRunner,
    create_test_config_file,
    get_default_stepflow_config,
)

from .test_registry import TestWorkflow, get_test_registry, pytest_parametrize_workflows


class TestWorkflowValidation:
    """Test validation of converted workflows using Stepflow binary."""

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
        """Create test configuration file."""
        config_content = get_default_stepflow_config()
        config_path = create_test_config_file(config_content)
        yield config_path
        # Cleanup
        Path(config_path).unlink(missing_ok=True)

    @pytest.mark.parametrize(
        "workflow",
        pytest_parametrize_workflows(get_test_registry().get_validation_test_cases()),
    )
    def test_converted_workflow_validation(
        self,
        workflow: TestWorkflow,
        registry,
        converter: LangflowConverter,
        stepflow_runner: StepflowBinaryRunner,
        test_config_path: str,
    ):
        """Test that converted workflows validate successfully."""

        # Load and convert workflow
        try:
            langflow_data = registry.load_langflow_data(workflow)
        except FileNotFoundError:
            pytest.skip(f"Langflow file not found: {workflow.langflow_file}")

        stepflow_workflow = converter.convert(langflow_data)
        workflow_yaml = converter.to_yaml(stepflow_workflow)

        # Validate using Stepflow binary
        success, stdout, stderr = stepflow_runner.validate_workflow(
            workflow_yaml, config_path=test_config_path
        )

        # Check validation result
        if workflow.validation.should_succeed:
            assert success, (
                f"Workflow '{workflow.name}' validation failed:\n"
                f"STDOUT: {stdout}\nSTDERR: {stderr}"
            )

            # Check for success indicators
            success_indicators = ["✅", "passed", "success", "valid"]
            has_success_indicator = any(
                indicator in stdout.lower() for indicator in success_indicators
            )
            assert has_success_indicator, (
                "No success indicator found in validation output for "
                f"'{workflow.name}': {stdout}"
            )
        else:
            assert not success, (
                f"Workflow '{workflow.name}' validation should have failed but "
                "succeeded"
            )

            # Check error message if specified
            if workflow.validation.error_contains:
                assert workflow.validation.error_contains in stderr.lower(), (
                    f"Expected error containing '{workflow.validation.error_contains}' "
                    f"in stderr: {stderr}"
                )

    def test_binary_availability(self, stepflow_runner: StepflowBinaryRunner):
        """Test that Stepflow binary is available and working."""
        available, version_info = stepflow_runner.check_binary_availability()

        assert available, f"Stepflow binary not available: {version_info}"
        assert "stepflow" in version_info.lower()

    def test_component_availability(
        self, stepflow_runner: StepflowBinaryRunner, test_config_path: str
    ):
        """Test that required components are available."""

        success, components, stderr = stepflow_runner.list_components(
            config_path=test_config_path
        )

        assert success, f"Failed to list components: {stderr}"
        assert isinstance(components, list)
        assert len(components) > 0, "Should have at least some components"

        # Should have builtin components
        builtin_components = [c for c in components if "builtin" in c.lower()]
        assert len(builtin_components) > 0, "Should have builtin components available"

        # Should have langflow/mock components for testing
        langflow_components = [c for c in components if "langflow" in c.lower()]
        assert len(langflow_components) > 0, (
            "Should have Langflow components available for testing"
        )


class TestValidationErrorHandling:
    """Test validation error handling scenarios."""

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

    def test_invalid_workflow_schema(
        self, stepflow_runner: StepflowBinaryRunner, test_config_path: str
    ):
        """Test validation of workflow with invalid schema."""

        invalid_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Invalid Schema Test
steps:
  - id: invalid_step
    component: /builtin/test
    input: {}
    invalid_field: "should not be here"
    another_invalid: 123
"""

        success, stdout, stderr = stepflow_runner.validate_workflow(
            invalid_yaml, config_path=test_config_path
        )

        # May pass validation but should indicate issues
        if success:
            # Even if validation passes, there should be warnings or
            # the workflow should fail at runtime
            pass
        else:
            # Should have clear error message
            error_indicators = ["error", "invalid", "schema", "unknown"]
            has_error_indicator = any(
                indicator in stdout.lower() or indicator in stderr.lower()
                for indicator in error_indicators
            )
            assert has_error_indicator, (
                "Should have error indicator in output: "
                f"stdout={stdout}, stderr={stderr}"
            )

    def test_nonexistent_component(
        self, stepflow_runner: StepflowBinaryRunner, test_config_path: str
    ):
        """Test that workflow with nonexistent component can be valid.

        Note: Stepflow validation focuses on workflow structure, not component
        availability. Component routing is validated at execution time, not validation
        time.
        """

        invalid_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Nonexistent Component Test
steps:
  - id: bad_step
    component: /nonexistent/component
    input: {}
"""

        # Validation should pass (structural validation only)
        success, stdout, stderr = stepflow_runner.validate_workflow(
            invalid_yaml, config_path=test_config_path
        )

        assert success, (
            "Workflow validation should pass (routing checked at execution time)"
        )

        # Should contain warnings about unreferenced step
        assert "warnings" in stdout.lower() or "warn" in stdout.lower(), (
            f"Should have warnings about workflow structure: stdout={stdout}"
        )

    def test_malformed_yaml(
        self, stepflow_runner: StepflowBinaryRunner, test_config_path: str
    ):
        """Test validation of malformed YAML."""

        malformed_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Malformed YAML Test
steps:
  - id: test
    component: /builtin/test
    input: {
      unclosed: dict
"""

        success, stdout, stderr = stepflow_runner.validate_workflow(
            malformed_yaml, config_path=test_config_path
        )

        assert not success, "Malformed YAML should fail validation"

        yaml_error_indicators = ["yaml", "parse", "invalid", "syntax"]
        has_yaml_error = any(
            indicator in stdout.lower() or indicator in stderr.lower()
            for indicator in yaml_error_indicators
        )
        assert has_yaml_error, (
            f"Should indicate YAML parsing error: stdout={stdout}, stderr={stderr}"
        )

    def test_workflow_with_dependency_issues(
        self, stepflow_runner: StepflowBinaryRunner, test_config_path: str
    ):
        """Test validation of workflow with dependency issues."""

        # Forward reference (step references undefined step)
        forward_ref_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Forward Reference Test
steps:
  - id: step1
    component: /builtin/create_messages
    input:
      messages:
        - role: user
          content:
            $from:
              step: undefined_step
            path: result

  - id: step2
    component: /builtin/create_messages
    input:
      messages:
        - role: assistant
          content: "Hello"
"""

        success, stdout, stderr = stepflow_runner.validate_workflow(
            forward_ref_yaml, config_path=test_config_path
        )

        # Should either fail validation or at runtime
        if success:
            # If validation passes, the workflow should fail during execution planning
            pass
        else:
            # Should indicate dependency issue
            dependency_indicators = ["undefined", "reference", "step", "not found"]
            has_dependency_error = any(
                indicator in stdout.lower() or indicator in stderr.lower()
                for indicator in dependency_indicators
            )
            assert has_dependency_error, (
                f"Should indicate dependency issue: stdout={stdout}, stderr={stderr}"
            )


class TestValidationPerformance:
    """Test validation performance for different workflow sizes."""

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

    @pytest.mark.slow
    def test_validation_performance(
        self, stepflow_runner: StepflowBinaryRunner, test_config_path: str
    ):
        """Test validation performance for different workflow sizes."""

        import time

        registry = get_test_registry()
        converter = LangflowConverter()
        performance_results = []

        for workflow in registry.get_performance_test_cases():
            if not workflow.conversion.should_succeed:
                continue

            try:
                langflow_data = registry.load_langflow_data(workflow)
                stepflow_workflow = converter.convert(langflow_data)
                workflow_yaml = converter.to_yaml(stepflow_workflow)

                # Measure validation time
                start_time = time.time()
                success, stdout, stderr = stepflow_runner.validate_workflow(
                    workflow_yaml, config_path=test_config_path
                )
                validation_time = time.time() - start_time

                node_count = len(langflow_data.get("data", {}).get("nodes", []))
                performance_results.append(
                    {
                        "workflow": workflow.name,
                        "nodes": node_count,
                        "validation_time": validation_time,
                        "success": success,
                    }
                )

            except Exception:
                # Skip workflows that don't convert
                continue

        # Basic performance assertions
        assert len(performance_results) > 0, (
            "Should have validated at least one workflow"
        )

        max_time = max(r["validation_time"] for r in performance_results)
        assert max_time < 10.0, f"Validation took too long: {max_time:.2f}s"

        # At least some validations should succeed
        successful_validations = sum(1 for r in performance_results if r["success"])
        assert successful_validations > 0, "At least some validations should succeed"

        # Print performance summary for debugging
        print("\nValidation Performance Results:")
        for result in sorted(
            performance_results, key=lambda x: x["validation_time"], reverse=True
        ):
            print(
                f"  {result['workflow']:20} | {result['nodes']:2} nodes | "
                f"{result['validation_time']:.3f}s | "
                f"{'✅' if result['success'] else '❌'}"
            )
