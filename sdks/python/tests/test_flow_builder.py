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

import pytest

from stepflow_py import (
    OnErrorDefault,
    OnErrorFail,
    OnErrorRetry,
    OnErrorSkip,
    StepReference,
    Value,
    WorkflowInput,
)
from stepflow_py.flow_builder import FlowBuilder


def test_basic_flow_builder():
    """Test creating a basic flow."""
    builder = FlowBuilder(name="test_flow", description="A test flow")

    # Add a step that uses workflow input
    step1 = builder.add_step(
        id="add_numbers",
        component="eval",
        input_data={"expr": "x + y", "x": Value.input().x, "y": Value.input().y},
    )

    # Add a step that uses the output of the previous step
    step2 = builder.add_step(
        id="double_result",
        component="eval",
        input_data={"expr": "result * 2", "result": Value.step(step1.id, "$")},
    )

    # Set the flow output
    builder.set_output({"final_result": Value.step(step2.id, "$")})

    # Build the flow
    flow = builder.build()

    # Check basic properties
    assert flow.name == "test_flow"
    assert flow.description == "A test flow"
    assert len(flow.steps or []) == 2

    # Check step 1
    steps = flow.steps or []
    step1_def = steps[0]
    assert step1_def.id == "add_numbers"
    assert step1_def.component == "eval"

    # Check step 2
    step2_def = steps[1]
    assert step2_def.id == "double_result"
    assert step2_def.component == "eval"


def test_step_references():
    """Test creating references to steps."""
    builder = FlowBuilder()

    # Add a step
    step1 = builder.add_step(
        id="test_step", component="/test/component", input_data={"value": 42}
    )

    # Test different ways to reference the step
    ref1 = step1  # Direct reference
    ref2 = step1.result  # Field reference
    ref3 = step1["result"]  # Indexing reference
    ref4 = step1.nested.field  # Nested field reference

    # Add steps that use these references
    builder.add_step(
        id="step1",
        component="/test/component",
        input_data={"direct": Value.step(step1.id, "$")},
    )
    builder.add_step(
        id="step2", component="/test/component", input_data={"field": ref2}
    )
    builder.add_step(
        id="step3", component="/test/component", input_data={"indexed": ref3}
    )
    builder.add_step(
        id="step4", component="/test/component", input_data={"nested": ref4}
    )

    # Set output to make the test complete
    builder.set_output({"result": Value.step(step1.id, "$")})

    flow = builder.build()
    assert len(flow.steps or []) == 5


def test_workflow_input_references():
    """Test creating references to workflow input."""
    builder = FlowBuilder()

    # Test different ways to reference workflow input
    input_ref = Value.input()
    field_ref = Value.input().field
    indexed_ref = Value.input()["field"]
    nested_ref = Value.input().nested.field

    # Add steps that use these references
    builder.add_step(
        id="step1", component="/test/component", input_data={"full_input": input_ref}
    )
    builder.add_step(
        id="step2", component="/test/component", input_data={"field": field_ref}
    )
    builder.add_step(
        id="step3", component="/test/component", input_data={"indexed": indexed_ref}
    )
    builder.add_step(
        id="step4", component="/test/component", input_data={"nested": nested_ref}
    )

    # Set output to make the test complete
    builder.set_output({"result": input_ref})

    flow = builder.build()
    assert len(flow.steps or []) == 4


def test_literal_values():
    """Test using literal values that won't be expanded."""
    builder = FlowBuilder()

    # Add a step with a literal value that contains $from
    builder.add_step(
        id="literal_step",
        component="/test/component",
        input_data={
            "literal_with_from": Value.literal(
                {"$from": "this should not be expanded"}
            ),
            "regular_value": "normal string",
        },
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    flow = builder.build()
    assert len(flow.steps or []) == 1


def test_error_handling():
    """Test error handling options."""
    builder = FlowBuilder()

    # Add steps with different error handling
    builder.add_step(
        id="fail_step",
        component="/test/component",
        input_data={"value": 1},
        on_error=OnErrorFail(action="fail"),
    )
    builder.add_step(
        id="skip_step",
        component="/test/component",
        input_data={"value": 2},
        on_error=OnErrorSkip(action="skip"),
    )
    builder.add_step(
        id="retry_step",
        component="/test/component",
        input_data={"value": 3},
        on_error=OnErrorRetry(action="retry"),
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    flow = builder.build()
    assert len(flow.steps or []) == 3

    # Check that error actions were set correctly
    fail_step = next(step for step in (flow.steps or []) if step.id == "fail_step")
    assert isinstance(fail_step.onError, OnErrorFail)
    assert fail_step.onError.action == "fail"

    skip_step = next(step for step in (flow.steps or []) if step.id == "skip_step")
    assert isinstance(skip_step.onError, OnErrorSkip)
    assert skip_step.onError.action == "skip"

    retry_step = next(step for step in (flow.steps or []) if step.id == "retry_step")
    assert isinstance(retry_step.onError, OnErrorRetry)
    assert retry_step.onError.action == "retry"


def test_error_default_handling():
    """Test OnErrorDefault with default values."""
    builder = FlowBuilder()

    # Add step with OnErrorDefault
    builder.add_step(
        id="default_step",
        component="/test/component",
        input_data={"value": 1},
        on_error=OnErrorDefault(action="useDefault", defaultValue="fallback_value"),
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    flow = builder.build()
    assert len(flow.steps or []) == 1

    # Check that OnErrorDefault was set correctly
    default_step = (flow.steps or [])[0]
    assert isinstance(default_step.onError, OnErrorDefault)
    assert default_step.onError.action == "useDefault"
    assert default_step.onError.defaultValue == "fallback_value"


def test_build_requires_output():
    """Test that build() fails when output hasn't been set."""
    builder = FlowBuilder()

    # Add a step but don't set output
    builder.add_step(
        id="test_step", component="/test/component", input_data={"value": 1}
    )

    # build() should fail without output
    with pytest.raises(ValueError, match="Flow output must be set before building"):
        builder.build()

    # After setting output, build() should succeed
    builder.set_output({"result": "success"})
    flow = builder.build()
    assert flow.output == {"result": "success"}


def test_step_ids():
    """Test that step IDs are set correctly."""
    builder = FlowBuilder()

    # Add steps with explicit IDs
    step1 = builder.add_step(
        id="custom_step_1", component="/test/component", input_data={"value": 1}
    )
    step2 = builder.add_step(
        id="custom_step_2", component="/test/component", input_data={"value": 2}
    )
    step3 = builder.add_step(
        id="custom_step_3", component="/test/component", input_data={"value": 3}
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    flow = builder.build()

    # Check that IDs were set correctly
    step_ids = [step.id for step in (flow.steps or [])]
    assert "custom_step_1" in step_ids
    assert "custom_step_2" in step_ids
    assert "custom_step_3" in step_ids

    # Check that handles have correct IDs
    assert step1.id == "custom_step_1"
    assert step2.id == "custom_step_2"
    assert step3.id == "custom_step_3"


def test_complex_nested_references():
    """Test complex nested references."""
    builder = FlowBuilder()

    # Add a step
    step1 = builder.add_step(
        id="nested_step_1",
        component="/test/component",
        input_data={"data": Value.input().config.settings},
    )

    # Add another step that references nested data from the first step
    step2 = builder.add_step(
        id="nested_step_2",
        component="/test/component",
        input_data={
            "prev_result": step1.output.nested.value,
            "input_ref": Value.input().another.nested.field,
        },
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    flow = builder.build()
    assert len(flow.steps or []) == 2


def test_get_references():
    """Test extracting references from a flow."""
    builder = FlowBuilder()

    # Add a step that uses workflow input
    step1 = builder.add_step(
        id="input_step",
        component="/test/component",
        input_data={"x": Value.input().x, "y": Value.input().nested.y},
    )

    # Add a step that references the first step
    step2 = builder.add_step(
        id="ref_step", component="/test/component", input_data={"result": step1.output}
    )

    # Set flow output that references step2
    builder.set_output({"final": step2.result})

    flow = builder.build()
    # Use the new load and instance method approach
    loaded_builder = FlowBuilder.load(flow)
    references = loaded_builder.get_references()

    # Should find references to workflow input and step outputs
    input_refs = [ref for ref in references if isinstance(ref, WorkflowInput)]
    step_refs = [ref for ref in references if isinstance(ref, StepReference)]

    assert len(input_refs) == 2  # input().x and input().nested.y
    assert len(step_refs) == 2  # step1.output and step2.result

    # Check specific references
    input_paths = [str(ref.path) for ref in input_refs]
    assert "$.x" in input_paths
    assert "$.nested.y" in input_paths

    step_ids = [ref.step_id for ref in step_refs]
    assert "input_step" in step_ids
    assert "ref_step" in step_ids


def test_get_step_references():
    """Test extracting references from a single step."""
    builder = FlowBuilder()

    # Add a step with multiple references
    step1 = builder.add_step(
        id="test_step",
        component="/test/component",
        input_data={
            "input_val": Value.input().value,
            "nested_input": Value.input().config.setting,
        },
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    flow = builder.build()
    references = builder.step("test_step").get_references()

    # Should find two workflow input references
    assert len(references) == 2
    assert all(isinstance(ref, WorkflowInput) for ref in references)

    paths = [str(ref.path) for ref in references]
    assert "$.value" in paths
    assert "$.config.setting" in paths


def test_get_references_with_skip_condition():
    """Test extracting references from skipIf conditions."""
    builder = FlowBuilder()

    # Add a step with a skipIf condition
    step1 = builder.add_step(
        id="skip_test",
        component="/test/component",
        input_data={"value": 42},
        skip_if=Value.input().skip_flag,
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    flow = builder.build()
    references = builder.step("skip_test").get_references()

    # Should find the skipIf reference
    assert len(references) == 1
    assert isinstance(references[0], WorkflowInput)
    assert str(references[0].path) == "$.skip_flag"


def test_get_references_with_literal_values():
    """Test that literal values don't produce references."""
    builder = FlowBuilder()

    # Add a step with literal values
    step1 = builder.add_step(
        id="literal_test",
        component="/test/component",
        input_data={
            "literal_dict": Value.literal({"$from": "should not be a reference"}),
            "normal_value": "just a string",
            "number": 42,
        },
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    flow = builder.build()
    references = builder.step("literal_test").get_references()

    # Should find no references
    assert len(references) == 0


def test_get_references_mixed_types():
    """Test reference extraction with mixed value types."""
    builder = FlowBuilder()

    # Add a step with mixed reference types
    step1 = builder.add_step(
        id="mixed_test",
        component="/test/component",
        input_data={
            "input_ref": Value.input().value,
            "literal_value": "string",
            "array_with_refs": [Value.input().item1, "literal", Value.input().item2],
            "nested_dict": {"ref": Value.input().nested.field, "literal": 123},
        },
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    flow = builder.build()
    references = builder.step("mixed_test").get_references()

    # Should find references from various nesting levels
    assert len(references) == 4
    assert all(isinstance(ref, WorkflowInput) for ref in references)

    paths = [str(ref.path) for ref in references]
    assert "$.value" in paths
    assert "$.item1" in paths
    assert "$.item2" in paths
    assert "$.nested.field" in paths


def test_flowbuilder_load():
    """Test FlowBuilder.load() method for loading existing flows."""
    # Create an original flow
    original_builder = FlowBuilder(name="test_flow", description="Test")

    step1 = original_builder.add_step(
        id="custom_step",
        component="/test/component",
        input_data={"input": Value.input().field, "literal": Value.literal("constant")},
    )

    original_builder.set_output({"result": Value.step(step1.id, "output")})
    original_flow = original_builder.build()

    # Load the flow using FlowBuilder.load()
    loaded_builder = FlowBuilder.load(original_flow)

    # Check that the loaded builder has the same properties
    assert loaded_builder.name == "test_flow"
    assert loaded_builder.description == "Test"
    assert len(loaded_builder.steps) == 1
    assert "custom_step" in loaded_builder.steps
    assert loaded_builder.steps["custom_step"].id == "custom_step"
    assert loaded_builder._output is not None

    # Check that step handles were recreated
    assert "custom_step" in loaded_builder._step_handles
    assert loaded_builder._step_handles["custom_step"].id == "custom_step"

    # Check that we can analyze the loaded flow
    references = loaded_builder.get_references()
    assert len(references) > 0

    # Check that we can continue building on the loaded flow
    step2 = loaded_builder.add_step(
        id="additional_step",
        component="test/component2",
        input_data={"prev": Value.step("custom_step", "result")},
    )

    # The new step should have the assigned ID
    assert step2.id == "additional_step"

    # Build the modified flow
    modified_flow = loaded_builder.build()
    assert len(modified_flow.steps or []) == 2


def test_flowbuilder_load_and_extend():
    """Test that FlowBuilder.load() correctly loads and can be extended."""
    # Create a flow with explicit step IDs
    original_builder = FlowBuilder()
    step1 = original_builder.add_step(id="step1", component="test/component1")
    step2 = original_builder.add_step(id="step2", component="test/component2")
    step3 = original_builder.add_step(id="custom_step", component="test/component3")

    # Set output to make the test complete
    original_builder.set_output({"result": "done"})

    original_flow = original_builder.build()

    # Load the flow
    loaded_builder = FlowBuilder.load(original_flow)

    # Add a new step with explicit ID
    new_step = loaded_builder.add_step(id="step4", component="test/component4")
    assert new_step.id == "step4"

    # Verify all steps are present
    final_flow = loaded_builder.build()
    step_ids = [step.id for step in (final_flow.steps or [])]
    assert "step1" in step_ids
    assert "step2" in step_ids
    assert "custom_step" in step_ids
    assert "step4" in step_ids


def test_new_object_oriented_api():
    """Test the new object-oriented API pattern with FlowBuilder.load()."""
    # Create a flow
    builder = FlowBuilder(name="analysis_test")

    step1 = builder.add_step(
        id="data_processor",
        component="data_processor",
        input_data={
            "source": Value.input().data_source,
            "config": Value.literal({"format": "json", "validate": True}),
        },
    )

    step2 = builder.add_step(
        id="analyzer",
        component="analyzer",
        input_data={"data": Value.step(step1.id, "processed_data")},
    )

    builder.set_output({"analysis": Value.step(step2.id, "results")})
    flow = builder.build()

    # Now demonstrate the new OO API for analysis

    # OLD WAY (static methods - no longer available):
    # references = FlowBuilder.get_references_from_flow(flow)
    # step_refs = FlowBuilder.get_step_references(flow.steps[0])

    # NEW WAY (object-oriented with load):
    analyzer = FlowBuilder.load(flow)

    # Analyze the entire flow
    all_references = analyzer.get_references()
    assert len(all_references) > 0

    # Analyze specific steps
    step1_refs = analyzer.step("data_processor").get_references()
    step2_refs = analyzer.step("analyzer").get_references()

    # Verify we found the expected references
    input_refs = [ref for ref in all_references if isinstance(ref, WorkflowInput)]
    step_refs = [ref for ref in all_references if isinstance(ref, StepReference)]

    assert len(input_refs) == 1  # input().data_source
    assert len(step_refs) == 2  # Two step references in step2 input and flow output
