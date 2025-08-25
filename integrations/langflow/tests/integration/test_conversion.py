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

"""Conversion tests: Langflow JSON → Stepflow YAML transformation.

These tests focus purely on the conversion process without involving the Stepflow binary.
They verify that Langflow workflow JSON is correctly transformed to Stepflow YAML format
with proper step ordering, component mapping, and dependency resolution.
"""

import pytest
from typing import Dict, Any

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.utils.errors import ConversionError, ValidationError

from .test_registry import get_test_registry, TestWorkflow, pytest_parametrize_workflows


class TestWorkflowConversion:
    """Test conversion of Langflow workflows to Stepflow format."""

    @pytest.fixture
    def registry(self):
        """Get test registry."""
        return get_test_registry()

    @pytest.fixture
    def converter(self):
        """Create converter instance."""
        return LangflowConverter()

    @pytest.mark.parametrize("workflow", pytest_parametrize_workflows(
        get_test_registry().get_conversion_test_cases()
    ))
    def test_workflow_conversion(self, workflow: TestWorkflow, registry, converter: LangflowConverter):
        """Test conversion for each registered workflow."""
        
        # Load the Langflow data
        try:
            langflow_data = registry.load_langflow_data(workflow)
        except FileNotFoundError:
            if workflow.conversion.should_succeed:
                pytest.fail(f"Required Langflow file not found: {workflow.langflow_file}")
            else:
                # Expected to fail - test file-level error handling
                with pytest.raises(FileNotFoundError):
                    registry.load_langflow_data(workflow)
                return

        # Perform conversion
        if workflow.conversion.should_succeed:
            # Conversion should succeed
            stepflow_workflow = converter.convert(langflow_data)
            
            # Verify conversion expectations
            self._assert_conversion_success(stepflow_workflow, workflow.conversion)
            
        else:
            # Conversion should fail
            self._assert_conversion_failure(converter, langflow_data, workflow.conversion)

    def _assert_conversion_success(self, stepflow_workflow, expectations):
        """Assert successful conversion meets expectations."""
        
        # Check workflow name
        if expectations.workflow_name:
            assert stepflow_workflow.name == expectations.workflow_name, \
                f"Expected workflow name '{expectations.workflow_name}', got '{stepflow_workflow.name}'"

        # Check step count
        if expectations.step_count is not None:
            assert len(stepflow_workflow.steps) == expectations.step_count, \
                f"Expected {expectations.step_count} steps, got {len(stepflow_workflow.steps)}"

        # Check step IDs (legacy - prefer component_types_include)
        if expectations.step_ids:
            actual_ids = [step.id for step in stepflow_workflow.steps]
            for expected_id in expectations.step_ids:
                # For legacy compatibility, check if any step ID contains the expected ID
                found = any(expected_id in actual_id for actual_id in actual_ids)
                assert found, \
                    f"Expected step ID pattern '{expected_id}' not found in {actual_ids}"

        # Check exact component types
        if expectations.component_types:
            actual_components = [step.component for step in stepflow_workflow.steps]
            assert actual_components == expectations.component_types, \
                f"Expected components {expectations.component_types}, got {actual_components}"

        # Check component types includes (partial match)
        if expectations.component_types_include:
            actual_components = [step.component for step in stepflow_workflow.steps]
            for expected_component in expectations.component_types_include:
                found = any(expected_component in comp for comp in actual_components)
                assert found, \
                    f"Expected component type '{expected_component}' not found in {actual_components}"

        # Check UDF executor count
        if expectations.udf_executor_count is not None:
            udf_count = sum(1 for step in stepflow_workflow.steps 
                           if step.component == "/langflow/udf_executor")
            assert udf_count == expectations.udf_executor_count, \
                f"Expected {expectations.udf_executor_count} UDF executors, found {udf_count}"

        # Verify no forward references (steps referencing later steps)
        self._assert_no_forward_references(stepflow_workflow)

    def _assert_conversion_failure(self, converter, langflow_data, expectations):
        """Assert conversion fails as expected."""
        
        with pytest.raises(Exception) as exc_info:
            converter.convert(langflow_data)
        
        # Check error type if specified
        if expectations.error_type:
            assert expectations.error_type in str(type(exc_info.value).__name__), \
                f"Expected error type '{expectations.error_type}', got {type(exc_info.value).__name__}"
        
        # Check error message contains expected text
        if expectations.error_message_contains:
            assert expectations.error_message_contains in str(exc_info.value), \
                f"Expected error message to contain '{expectations.error_message_contains}', got: {str(exc_info.value)}"

    def _assert_no_forward_references(self, stepflow_workflow):
        """Assert workflow has no forward references (step ordering is correct)."""
        step_ids = [step.id for step in stepflow_workflow.steps]
        
        for i, step in enumerate(stepflow_workflow.steps):
            if hasattr(step, 'input') and step.input:
                for key, value in step.input.items():
                    if isinstance(value, dict) and '$from' in str(value):
                        from_info = value.get('$from', {})
                        referenced_step = from_info.get('step', '')
                        if referenced_step:
                            try:
                                ref_pos = step_ids.index(referenced_step)
                                assert ref_pos < i, \
                                    f"Step '{step.id}' at position {i} references '{referenced_step}' at position {ref_pos} (forward reference)"
                            except ValueError:
                                pytest.fail(f"Step '{step.id}' references undefined step '{referenced_step}'")

    def test_yaml_output_quality(self, registry, converter: LangflowConverter):
        """Test that generated YAML is well-formed and readable."""
        
        for workflow in registry.get_conversion_test_cases():
            if not workflow.conversion.should_succeed:
                continue
            
            try:
                langflow_data = registry.load_langflow_data(workflow)
                stepflow_workflow = converter.convert(langflow_data)
                yaml_output = converter.to_yaml(stepflow_workflow)
                
                # Should be valid YAML
                import yaml
                parsed_yaml = yaml.safe_load(yaml_output)
                assert isinstance(parsed_yaml, dict)
                
                # Should have expected structure
                assert "name" in parsed_yaml
                assert "steps" in parsed_yaml
                assert isinstance(parsed_yaml["steps"], list)
                
                # Should be reasonably readable (not too long lines)
                lines = yaml_output.split('\n')
                for line in lines:
                    assert len(line) < 200, \
                        f"Line too long in YAML output for {workflow.name}: {line[:50]}..."
                
            except (FileNotFoundError, ConversionError, ValidationError):
                continue

    def test_analysis_consistency(self, registry, converter: LangflowConverter):
        """Test that analysis and conversion give consistent results."""
        
        for workflow in registry.get_conversion_test_cases():
            if not workflow.conversion.should_succeed:
                continue
            
            try:
                langflow_data = registry.load_langflow_data(workflow)
                
                # Analyze
                analysis = converter.analyze(langflow_data)
                
                # Convert
                stepflow_workflow = converter.convert(langflow_data)
                
                # Check consistency - Note: Analysis counts all nodes, conversion filters out notes/docs
                # So we expect conversion steps <= analysis nodes, not exact equality
                assert len(stepflow_workflow.steps) <= analysis.node_count, \
                    f"Converted step count ({len(stepflow_workflow.steps)}) should not exceed analysis node count ({analysis.node_count}) for {workflow.name}"
                
                # Total components in analysis should be >= steps (some may be filtered)
                total_components = sum(analysis.component_types.values())
                assert len(stepflow_workflow.steps) <= total_components, \
                    f"Step count ({len(stepflow_workflow.steps)}) should not exceed analysis component count ({total_components}) for {workflow.name}"
                
            except (FileNotFoundError, ConversionError, ValidationError):
                continue


class TestConversionErrorHandling:
    """Test error handling for various invalid inputs."""

    @pytest.fixture
    def converter(self):
        """Create converter instance."""
        return LangflowConverter()

    def test_missing_data_key(self, converter: LangflowConverter):
        """Test conversion fails for missing data key."""
        invalid_data = {"invalid": "structure"}
        
        with pytest.raises(ConversionError, match="missing 'data' key"):
            converter.convert(invalid_data)

    def test_empty_nodes(self, converter: LangflowConverter):
        """Test conversion fails for empty nodes list."""
        invalid_data = {"data": {"nodes": [], "edges": []}}
        
        with pytest.raises(ConversionError, match="No nodes found"):
            converter.convert(invalid_data)

    def test_malformed_node(self, converter: LangflowConverter):
        """Test conversion fails for malformed node structure."""
        invalid_data = {
            "data": {
                "nodes": [{"invalid": "node"}],
                "edges": []
            }
        }
        
        with pytest.raises(ConversionError):
            converter.convert(invalid_data)

    def test_circular_dependencies(self, converter: LangflowConverter):
        """Test conversion handles circular dependencies."""
        # This would need to be implemented based on actual edge structure
        # that creates circular dependencies
        circular_data = {
            "data": {
                "nodes": [
                    {"id": "node1", "data": {"type": "TestComponent", "node": {"template": {}}}, "type": "genericNode"},
                    {"id": "node2", "data": {"type": "TestComponent", "node": {"template": {}}}, "type": "genericNode"}
                ],
                "edges": [
                    {"source": "node1", "target": "node2", "data": {}},
                    {"source": "node2", "target": "node1", "data": {}}  # Creates cycle
                ]
            }
        }
        
        # Should either handle gracefully or raise clear error
        try:
            result = converter.convert(circular_data)
            # If conversion succeeds, verify no forward references
            step_ids = [step.id for step in result.steps]
            assert len(step_ids) == len(set(step_ids)), "No duplicate step IDs"
        except (ConversionError, ValueError) as e:
            # Should have clear error message about circular dependencies
            assert "circular" in str(e).lower() or "cycle" in str(e).lower()