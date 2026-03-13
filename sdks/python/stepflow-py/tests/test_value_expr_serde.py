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

"""Round-trip serialization tests for flows with value expressions.

These tests verify that flows containing all varieties of value expressions
can be correctly built, serialized, and deserialized without errors.

The primary regression being tested is: the old oneOf schema for ValueExpr
caused "Multiple matches found when deserializing" errors when plain dict
step inputs (like {"message": "hello", "model": "gpt-4"}) were deserialized.

The fix: ValueExpr schema is now a description-only schema (admitting any JSON
value). The Python SDK no longer generates a ValueExpr wrapper class — instead,
Step.input and Flow.output are typed as Any, and FlowBuilder produces plain
JSON-compatible dicts.

This bug manifested in the CLI `run` command which serializes a flow to YAML
and back to a dict before calling client.store_flow(dict), triggering
deserialization for every step input.
"""

import msgspec

from stepflow_py.worker import Flow
from stepflow_py.worker.flow_builder import FlowBuilder
from stepflow_py.worker.value import Value


def _roundtrip(flow: Flow) -> Flow:
    """Serialize a flow to dict and deserialize it back."""
    flow_dict = msgspec.to_builtins(flow)
    return msgspec.convert(flow_dict, Flow)


def _get_steps(flow: Flow) -> list:
    """Get steps list from a flow, handling UNSET."""
    steps = flow.steps if flow.steps is not msgspec.UNSET else []
    return list(steps or [])


class TestFlowDictRoundTrip:
    """A flow built with FlowBuilder can round-trip through dict serialization.

    This simulates the CLI path:
    1. LangflowConverter.convert() → Flow object
    2. msgspec.to_builtins(flow) → plain dict
    3. msgspec.convert(dict, Flow) → should work without errors
    """

    def test_simple_step_input_dict(self):
        """Plain dict step input round-trips without errors."""
        builder = FlowBuilder()
        builder.add_step(
            id="echo_step",
            component="/builtin/echo",
            input_data={"message": "hello", "model": "gpt-4"},
        )
        step = builder.step("echo_step")
        builder.set_output(step.result)
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        assert reconstructed is not None
        assert len(_get_steps(reconstructed)) == 1

    def test_step_with_nested_template_dict(self):
        """Complex nested dict step input (like Langflow templates) round-trips."""
        complex_input = {
            "template": {
                "_type": "Component",
                "api_key": {"value": "", "load_from_db": True},
                "model_name": {"value": "gpt-4o-mini"},
                "code": {"value": "class MyComponent: pass"},
            },
            "outputs": [{"name": "text_output", "method": "text_response"}],
            "selected_output": "text_output",
        }

        builder = FlowBuilder()
        builder.add_step(
            id="langflow_step",
            component="/langflow/MyComponent",
            input_data=complex_input,
        )
        step = builder.step("langflow_step")
        builder.set_output(step.result)
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        assert reconstructed is not None
        step0 = _get_steps(reconstructed)[0]
        assert step0.input is not None
        assert step0.input["template"]["_type"] == "Component"

    def test_flow_with_step_references(self):
        """Flow with step references round-trips without errors."""
        builder = FlowBuilder()
        step1 = builder.add_step(
            id="step1",
            component="/builtin/fetch",
            input_data={"url": Value.input("url")},
        )
        step2 = builder.add_step(
            id="step2",
            component="/builtin/process",
            input_data={"data": step1.result, "config": {"timeout": 30}},
        )
        builder.set_output(step2.result)
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        assert reconstructed is not None
        assert len(_get_steps(reconstructed)) == 2

        # Verify the step references are preserved
        step2_input = _get_steps(reconstructed)[1].input
        assert step2_input is not None
        assert step2_input["data"] == {"$step": "step1", "path": "$.result"}
        assert step2_input["config"] == {"timeout": 30}

    def test_flow_with_expression_types(self):
        """Flow with all expression types round-trips correctly."""
        from stepflow_py.worker import ValueExpr

        builder = FlowBuilder()
        builder.add_step(
            id="step1",
            component="/test/component",
            input_data={
                "step_ref": {"$step": "previous_step"},
                "input_ref": {"$input": "$.message"},
                "variable_ref": {"$variable": "api_key"},
                "literal_expr": {"$literal": {"$step": "not_a_ref"}},
                "coalesce_expr": ValueExpr.coalesce(
                    {"$variable": "config"},
                    {"$literal": {"default": True}},
                ),
            },
        )
        builder.set_output({"result": {"$step": "step1"}})
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        assert reconstructed is not None

        step_input = _get_steps(reconstructed)[0].input
        assert step_input["step_ref"] == {"$step": "previous_step"}
        assert step_input["input_ref"] == {"$input": "$.message"}
        assert step_input["variable_ref"] == {"$variable": "api_key"}
        assert step_input["literal_expr"] == {"$literal": {"$step": "not_a_ref"}}
        assert step_input["coalesce_expr"]["$coalesce"][0] == {"$variable": "config"}


class TestStepInputFormats:
    """Various step input formats serialize and deserialize correctly.

    Regression: before the fix, ANY dict step input would fail deserialization
    with "Multiple matches found when deserializing the JSON string into ValueExpr".
    """

    def test_plain_string_input(self):
        """Plain string step input serializes correctly."""
        builder = FlowBuilder()
        builder.add_step(
            id="step1",
            component="/test/component",
            input_data="just a string",
        )
        builder.set_output({"result": "done"})
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        assert _get_steps(reconstructed)[0].input == "just a string"

    def test_plain_dict_input(self):
        """Plain dict step input serializes correctly."""
        builder = FlowBuilder()
        builder.add_step(
            id="step1",
            component="/test/component",
            input_data={"message": "hello", "count": 3, "enabled": True},
        )
        builder.set_output({"result": "done"})
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        step_input = _get_steps(reconstructed)[0].input
        assert step_input == {"message": "hello", "count": 3, "enabled": True}

    def test_step_ref_input(self):
        """Step reference input serializes correctly."""
        builder = FlowBuilder()
        builder.add_step(
            id="step1",
            component="/test/component",
            input_data={"$step": "previous_step", "path": "result"},
        )
        builder.set_output({"result": "done"})
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        step_input = _get_steps(reconstructed)[0].input
        assert step_input == {"$step": "previous_step", "path": "result"}

    def test_literal_escaped_input(self):
        """Literal-escaped input serializes correctly."""
        builder = FlowBuilder()
        builder.add_step(
            id="step1",
            component="/test/component",
            input_data={"$literal": {"$step": "this_is_a_literal"}},
        )
        builder.set_output({"result": "done"})
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        step_input = _get_steps(reconstructed)[0].input
        assert step_input == {"$literal": {"$step": "this_is_a_literal"}}

    def test_null_input(self):
        """Null step input serializes correctly."""
        builder = FlowBuilder()
        builder.add_step(
            id="step1",
            component="/test/component",
            input_data=None,
        )
        builder.set_output({"result": "done"})
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        # None input should be absent or None in the reconstructed step
        step = _get_steps(reconstructed)[0]
        assert step.input is None or step.input is msgspec.UNSET


class TestFlowOutputFormats:
    """Flow output can use all expression formats."""

    def test_output_as_step_ref(self):
        """Flow output as step reference serializes correctly."""
        builder = FlowBuilder()
        builder.add_step(id="step1", component="/test/component")
        builder.set_output({"$step": "step1"})
        flow = builder.build()

        assert flow.output == {"$step": "step1"}

        reconstructed = _roundtrip(flow)
        assert reconstructed.output == {"$step": "step1"}

    def test_output_as_structured_dict(self):
        """Flow output as structured dict with references serializes correctly."""
        builder = FlowBuilder()
        step1 = builder.add_step(id="step1", component="/test/component")
        step2 = builder.add_step(id="step2", component="/test/component")
        builder.set_output(
            {
                "result": Value(step1.result),
                "metadata": Value(step2.result),
            }
        )
        flow = builder.build()

        reconstructed = _roundtrip(flow)
        assert reconstructed.output["result"] == {
            "$step": "step1",
            "path": "$.result",
        }
