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

"""Test variables schema functionality in FlowBuilder."""

from stepflow_py.worker import FlowBuilder, Value


def test_variables_schema_from_dict():
    """Test setting variables schema from a dictionary."""
    builder = FlowBuilder(name="test_flow")

    variables_schema = {
        "type": "object",
        "properties": {
            "api_key": {
                "type": "string",
                "is_secret": True,
                "description": "OpenAI API key",
            },
            "temperature": {
                "type": "number",
                "default": 0.7,
                "minimum": 0,
                "maximum": 2,
            },
            "max_tokens": {"type": "integer", "default": 100, "minimum": 1},
        },
        "required": ["api_key"],
    }

    builder.set_variables_schema(variables_schema)

    # Add a simple step to make the flow valid
    builder.add_step(
        id="test_step",
        component="/builtin/eval",
        input_data={"code": Value.literal("return 'test'")},
    )

    builder.set_output(Value.step("test_step"))

    flow = builder.build()

    assert flow.schemas is not None
    assert flow.schemas.variables is not None


def test_variables_schema_from_simple_dict():
    """Test setting variables schema from a simple dictionary."""
    builder = FlowBuilder(name="test_flow")

    variables_schema = {
        "type": "object",
        "properties": {"simple_var": {"type": "string"}},
    }

    builder.set_variables_schema(variables_schema)

    # Add a simple step to make the flow valid
    builder.add_step(
        id="test_step",
        component="/builtin/eval",
        input_data={"code": Value.literal("return 'test'")},
    )

    builder.set_output(Value.step("test_step"))

    flow = builder.build()

    assert flow.schemas is not None
    assert flow.schemas.variables is not None


def test_variables_schema_load_flow():
    """Test that variables schema is preserved when loading a flow."""
    # Create a flow with variables schema
    builder = FlowBuilder(name="original_flow")

    variables_schema = {
        "type": "object",
        "properties": {"config_value": {"type": "string", "default": "default_value"}},
    }

    builder.set_variables_schema(variables_schema)

    builder.add_step(
        id="test_step",
        component="/builtin/eval",
        input_data={"code": Value.literal("return 'test'")},
    )

    builder.set_output(Value.step("test_step"))

    original_flow = builder.build()

    # Load the flow into a new builder
    loaded_builder = FlowBuilder.load(original_flow)

    # Variables schema should be preserved from the original flow
    assert loaded_builder.variables_schema is not None
    assert loaded_builder.variables_schema == variables_schema

    # Build the loaded flow
    rebuilt_flow = loaded_builder.build()

    assert rebuilt_flow.schemas is not None
    assert rebuilt_flow.schemas.variables is not None


def test_variables_schema_method_chaining():
    """Test that set_variables_schema supports method chaining."""
    builder = FlowBuilder(name="test_flow")

    result = builder.set_variables_schema(
        {"type": "object", "properties": {"test_var": {"type": "string"}}}
    )

    # Should return the builder itself for chaining
    assert result is builder


def test_variables_schema_none_by_default():
    """Test that variables schema is None by default."""
    builder = FlowBuilder(name="test_flow")

    # Add a simple step to make the flow valid
    builder.add_step(
        id="test_step",
        component="/builtin/eval",
        input_data={"code": Value.literal("return 'test'")},
    )

    builder.set_output(Value.step("test_step"))

    flow = builder.build()

    # No schemas should be set if no input/output/variables schemas were provided
    assert flow.schemas is None
