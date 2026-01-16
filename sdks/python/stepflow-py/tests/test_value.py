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

from stepflow_py.api.models import LiteralExpr
from stepflow_py.worker.flow_builder import FlowBuilder
from stepflow_py.worker.value import JsonPath, StepReference, Value, WorkflowInput


def test_value_api_methods():
    """Test the new cleaner Value API methods."""
    # Test literal creation
    literal_value = Value.literal({"$from": "test"})
    assert isinstance(literal_value, Value)
    assert isinstance(literal_value._value, LiteralExpr)
    assert literal_value._value.literal == {"$from": "test"}

    # Test step reference creation
    step_value = Value.step("step1", "output.field")
    assert isinstance(step_value, Value)
    assert isinstance(step_value._value, StepReference)
    assert step_value._value.step_id == "step1"
    assert (
        str(step_value._value.path) == "output.field"
    )  # This is set directly, not through JsonPath

    # Test workflow input creation
    input_value = Value.input("input.field")
    assert isinstance(input_value, Value)
    assert isinstance(input_value._value, WorkflowInput)
    assert (
        str(input_value._value.path) == "input.field"
    )  # This is set directly, not through JsonPath

    # Test Value.input() method
    input_value_root = Value.input()
    assert isinstance(input_value_root, Value)
    assert isinstance(input_value_root._value, WorkflowInput)
    assert str(input_value_root._value.path) == "$"

    # Test Value.input() attribute access
    field_value = Value.input().field
    assert isinstance(field_value, Value)
    assert isinstance(field_value._value, WorkflowInput)
    assert str(field_value._value.path) == "$.field"

    # Test that FlowBuilder has instance method for getting references
    builder = FlowBuilder()
    builder.add_step(
        id="test_step",
        component="test/component",
        input_data={"input": Value.input().field},
    )

    # Set output to make the test complete
    builder.set_output({"result": "done"})

    # Test instance method
    references_instance = builder.get_references()

    # Test the load method approach
    flow = builder.build()
    references_loaded = FlowBuilder.load(flow).get_references()

    # Both should return the same references
    assert len(references_instance) == len(references_loaded)
    assert str(references_instance[0].path) == str(references_loaded[0].path)


def test_value_class_basic():
    """Test basic Value class functionality."""
    # Test creating literal values
    literal_val = Value.literal({"key": "value"})
    assert isinstance(literal_val, Value)
    assert isinstance(literal_val._value, LiteralExpr)
    assert literal_val._value.literal == {"key": "value"}

    # Test creating step references
    step_val = Value.step("step1", "output.field")
    assert isinstance(step_val, Value)
    assert isinstance(step_val._value, StepReference)
    assert step_val._value.step_id == "step1"
    assert str(step_val._value.path) == "output.field"

    # Test creating workflow input references
    input_val = Value.input("input.field")
    assert isinstance(input_val, Value)
    assert isinstance(input_val._value, WorkflowInput)
    assert str(input_val._value.path) == "input.field"


def test_value_class_constructor():
    """Test Value class constructor with various inputs."""
    # Test with primitive values
    val1 = Value("hello")
    assert val1._value == "hello"

    val2 = Value(42)
    assert val2._value == 42

    val3 = Value({"key": "value"})
    assert val3._value == {"key": "value"}

    # Test with StepReference
    step_ref = StepReference("step1", JsonPath().with_field("output"))
    val4 = Value(step_ref)
    assert val4._value == step_ref

    # Test with WorkflowInput
    input_ref = WorkflowInput(JsonPath().with_field("input").with_field("field"))
    val5 = Value(input_ref)
    assert val5._value == input_ref

    # Test with LiteralExpr (escaped literal)
    escaped = LiteralExpr(literal="test")
    val6 = Value(escaped)
    assert val6._value == escaped

    # Test with another Value (unwrapping)
    val7 = Value(val1)
    assert val7._value == "hello"


def test_value_class_nested_references():
    """Test that Value class supports nested references."""
    # Create a step reference Value
    step_val = Value.step("step1")

    # Test nested field access
    nested_val = step_val.output
    assert isinstance(nested_val, Value)
    assert isinstance(nested_val._value, StepReference)
    assert nested_val._value.step_id == "step1"
    assert str(nested_val._value.path) == "$.output"

    # Test indexing access
    indexed_val = step_val["result"]
    assert isinstance(indexed_val, Value)
    assert isinstance(indexed_val._value, StepReference)
    assert indexed_val._value.step_id == "step1"
    assert str(indexed_val._value.path) == '$["result"]'

    # Test workflow input
    input_val = Value.input()
    nested_input = input_val.config.setting
    assert isinstance(nested_input, Value)
    assert isinstance(nested_input._value, WorkflowInput)
    assert str(nested_input._value.path) == "$.config.setting"


def test_value_class_in_flow_builder():
    """Test using Value class with FlowBuilder."""
    builder = FlowBuilder()

    # Test using Value objects in step input
    step1 = builder.add_step(
        id="value_test",
        component="test/component",
        input_data={
            "literal_value": Value.literal({"key": "value"}),
            "step_ref": Value.step("previous_step", "output"),
            "input_ref": Value.input("config.setting"),
            "mixed_dict": {
                "literal": Value.literal("test"),
                "reference": Value.input("input.field"),
            },
        },
    )

    # Test using Value in set_output
    builder.set_output(
        {
            "result": Value.step(step1.id, "output"),
            "constant": Value.literal("done"),
        }
    )

    flow = builder.build()
    assert len(flow.steps or []) == 1

    # Extract references to make sure they work
    references = FlowBuilder.load(flow).get_references()
    step_refs = [ref for ref in references if isinstance(ref, StepReference)]
    input_refs = [ref for ref in references if isinstance(ref, WorkflowInput)]

    assert len(step_refs) >= 1  # At least the step reference in output
    assert len(input_refs) >= 2  # At least config.setting and input.field


def test_value_class_error_handling():
    """Test error handling in Value class."""
    # Test that indexing non-reference values raises TypeError
    literal_val = Value.literal({"key": "value"})
    with pytest.raises(TypeError):
        literal_val["key"]

    # Test that attribute access on non-reference values raises AttributeError
    with pytest.raises(AttributeError):
        literal_val.nonexistent


def test_value_input_method():
    """Test that Value.input() method works as expected."""
    # Value.input() should return a Value that references workflow input
    assert isinstance(Value.input(), Value)

    # It should support attribute access for nested fields
    nested = Value.input().config.database.url
    assert isinstance(nested, Value)
    assert isinstance(nested._value, WorkflowInput)
    assert str(nested._value.path) == "$.config.database.url"

    # It should support indexing
    indexed = Value.input()["settings"]["theme"]
    assert isinstance(indexed, Value)
    assert isinstance(indexed._value, WorkflowInput)
    assert str(indexed._value.path) == '$["settings"]["theme"]'

    # Mixed access should work
    mixed = Value.input().user["preferences"].theme
    assert isinstance(mixed, Value)
    assert isinstance(mixed._value, WorkflowInput)
    assert str(mixed._value.path) == '$.user["preferences"].theme'


def test_json_path_class():
    """Test the JsonPath class for consistent path handling."""
    # Test empty path
    empty_path = JsonPath()
    assert str(empty_path) == "$"
    assert empty_path.fragments == ["$"]

    # Test field access from root using with_field (immutable)
    field_path = empty_path.with_field("field")
    assert str(field_path) == "$.field"
    assert field_path.fragments == ["$", ".field"]
    # Original should be unchanged
    assert str(empty_path) == "$"

    # Test chained field access
    nested_path = field_path.with_field("nested")
    assert str(nested_path) == "$.field.nested"
    assert nested_path.fragments == ["$", ".field", ".nested"]

    # Test index access from root using with_index (immutable)
    index_path = JsonPath().with_index("key")
    assert str(index_path) == '$["key"]'
    assert index_path.fragments == ["$", '["key"]']

    # Test numeric index
    numeric_index = JsonPath().with_index(0)
    assert str(numeric_index) == "$[0]"
    assert numeric_index.fragments == ["$", "[0]"]

    # Test mixed access using immutable methods
    mixed_path = JsonPath().with_field("field").with_index("key").with_field("value")
    assert str(mixed_path) == '$.field["key"].value'
    assert mixed_path.fragments == ["$", ".field", '["key"]', ".value"]

    # Test mutating methods
    mutable_path = JsonPath()
    mutable_path.push_field("config")
    mutable_path.push_index("env")
    mutable_path.push_field("setting")
    assert str(mutable_path) == '$.config["env"].setting'

    # Test path consistency with actual usage using proper FlowBuilder API
    from stepflow_py.worker.flow_builder import FlowBuilder

    builder = FlowBuilder()
    step_handle = builder.add_step(id="test_step", component="test/component")
    step_ref = step_handle.field["key"][0].nested
    assert str(step_ref.path) == '$.field["key"][0].nested'

    workflow_input = WorkflowInput()
    input_ref = workflow_input.config["env"].setting
    assert str(input_ref.path) == '$.config["env"].setting'


def test_json_path_consistency():
    """Test that all path-handling classes use JsonPath consistently."""
    # All these should produce consistent JSON Path format

    # StepHandle -> StepReference
    from stepflow_py.worker.flow_builder import FlowBuilder

    builder = FlowBuilder()
    handle = builder.add_step(id="step1", component="test/component")
    step_ref1 = handle.output.data[0].field
    assert str(step_ref1.path) == "$.output.data[0].field"

    # Direct StepReference operations
    step_ref2 = StepReference("step1")
    step_ref2_nested = step_ref2.output.data[0].field
    assert str(step_ref2_nested.path) == "$.output.data[0].field"

    # WorkflowInput operations
    input_ref = WorkflowInput()
    input_nested = input_ref.config["database"].url
    assert str(input_nested.path) == '$.config["database"].url'

    # Value.input() operations (should delegate to WorkflowInput)
    value_input = Value.input().config["database"].url
    assert str(value_input._value.path) == '$.config["database"].url'

    # All should be consistent
    assert str(step_ref1.path) == str(step_ref2_nested.path)
    assert str(input_nested.path) == str(value_input._value.path)
