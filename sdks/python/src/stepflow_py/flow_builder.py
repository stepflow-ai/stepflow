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

"""Flow builder for creating StepFlow workflows programmatically."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .generated_protocol import (
    Component,
    ErrorAction,
    EscapedLiteral,
    Flow,
    Reference,
    Schema,
    Step,
    ValueTemplate,
    WorkflowReference,
)
from .generated_protocol import (
    StepReference as GeneratedStepReference,
)
from .generated_protocol import (
    Value as GeneratedValue,
)
from .value import (
    JsonPath,
    StepReference,
    Valuable,
    Value,
    WorkflowInput,
)


@dataclass
class StepHandle:
    """Handle for interacting with a step.

    Allows creating references to the step's output or analyzing
    the references within the step.
    """

    step: Step
    builder: FlowBuilder

    @property
    def id(self) -> str:
        """Get the step ID."""
        return self.step.id

    def get_references(self) -> list[StepReference | WorkflowInput]:
        """Extract all references used in this step."""
        return self.builder._get_step_references(self.step)

    def __getitem__(self, key: str | int) -> StepReference:
        """Create a reference to a specific path in this step's output."""
        path = JsonPath().with_index(key)
        return StepReference(self.step.id, path)

    def __getattr__(self, name: str) -> StepReference:
        """Create a reference to a field in this step's output."""
        path = JsonPath().with_field(name)
        return StepReference(self.step.id, path)


class FlowBuilder:
    """Builder for creating StepFlow workflows.

    This class provides methods for building workflows programmatically using the
    Value API. All input_data parameters accept Valuable types (Value, StepReference,
    WorkflowInput, etc.)

    Recommended usage:
    - Use Value.literal() for creating literal values: Value.literal({"key": "value"})
    - Use Value.step() for step references: Value.step("step1", "output")
    - Use Value.input() for input references: Value.input("config.setting")
    - Use Value() constructor for converting any Valuable to a Value
    - Use builder.step(name) to access steps for analysis and reference creation
    - Use builder.get_references() for analyzing flows
    - Use FlowBuilder.load(flow) to create a builder from an existing flow for analysis

    Examples:
        # Creating a new flow
        builder = FlowBuilder()
        step = builder.add_step(
            step_id="my_step",
            component="my_component",
            input_data={
                "input_field": Value.input().field,
                "literal": Value.literal({"$from": "raw_value"}),
                "step_ref": Value.step("previous_step", "output"),
                "mixed": Value({"nested": Value.input().config})
            }
        )
        references = builder.get_references()

        # Loading and analyzing an existing flow
        loaded_builder = FlowBuilder.load(existing_flow)
        references = loaded_builder.get_references()
        step_refs = loaded_builder.step("step_name").get_references()
    """

    def __init__(
        self,
        name: str | None = None,
        description: str | None = None,
        version: str | None = None,
    ):
        self.name = name
        self.description = description
        self.version = version
        self.input_schema: Schema | None = None
        self.output_schema: Schema | None = None
        self.steps: dict[str, Step] = {}
        self._step_handles: dict[str, StepHandle] = {}
        self._output: ValueTemplate | None = None

    @classmethod
    def load(cls, flow: Flow) -> FlowBuilder:
        """Create a FlowBuilder from an existing Flow.

        This allows you to load an existing flow and analyze or modify it.

        Example:
            builder = FlowBuilder.load(existing_flow)
            references = builder.get_references()
            step_refs = builder.step("step_name").get_references()
        """
        builder = cls(
            name=flow.name, description=flow.description, version=flow.version
        )
        builder.input_schema = flow.inputSchema
        builder.output_schema = flow.outputSchema
        builder._output = flow.output

        # Recreate steps as dict and step handles
        for step in flow.steps or []:
            builder.steps[step.id] = step
            builder._step_handles[step.id] = StepHandle(step, builder)

        return builder

    def step(self, step_id: str) -> StepHandle:
        """Get a step by name for analysis and reference creation."""
        if step_id not in self.steps:
            raise KeyError(f"Step '{step_id}' not found")
        return StepHandle(self.steps[step_id], self)

    def set_input_schema(self, schema: dict[str, Any] | Schema) -> FlowBuilder:
        """Set the input schema for the flow."""
        if isinstance(schema, dict):
            self.input_schema = Schema(**schema)
        else:
            self.input_schema = schema
        return self

    def set_output_schema(self, schema: dict[str, Any] | Schema) -> FlowBuilder:
        """Set the output schema for the flow."""
        if isinstance(schema, dict):
            self.output_schema = Schema(**schema)
        else:
            self.output_schema = schema
        return self

    def add_step(
        self,
        *,
        id: str,
        component: Component,
        input_data: Valuable | None = None,
        input_schema: dict[str, Any] | Schema | None = None,
        output_schema: dict[str, Any] | Schema | None = None,
        skip_if: StepReference | WorkflowInput | Value | None = None,
        on_error: ErrorAction | None = None,
    ) -> StepHandle:
        """Add a step to the flow."""
        # Component is now just a string, no conversion needed
        # component is already the correct type

        # Convert input data to ValueTemplate
        input_template = self._convert_to_value_template(input_data)

        # Convert schemas
        input_schema_obj = None
        if input_schema is not None:
            if isinstance(input_schema, dict):
                input_schema_obj = Schema(**input_schema)
            else:
                input_schema_obj = input_schema

        output_schema_obj = None
        if output_schema is not None:
            if isinstance(output_schema, dict):
                output_schema_obj = Schema(**output_schema)
            else:
                output_schema_obj = output_schema

        # Convert skip_if to Expr
        skip_if_expr = None
        if skip_if is not None:
            if isinstance(skip_if, Value):
                skip_if_ref = skip_if._value
                if isinstance(skip_if_ref, StepReference | WorkflowInput):
                    skip_if_expr = self._convert_reference_to_expr(skip_if_ref)
                else:
                    raise ValueError(
                        "skip_if Value must contain a StepReference or WorkflowInput"
                    )
            else:
                skip_if_expr = self._convert_reference_to_expr(skip_if)

        # on_error is already an ErrorAction or None
        on_error_action = on_error

        # Create the step
        step = Step(
            id=id,
            component=component,
            input=input_template,
            inputSchema=input_schema_obj,
            outputSchema=output_schema_obj,
            skipIf=skip_if_expr,
            onError=on_error_action,
        )

        self.steps[id] = step

        # Create and store handle
        handle = StepHandle(step, self)
        self._step_handles[id] = handle

        return handle

    def set_output(self, output_data: Valuable) -> FlowBuilder:
        """Set the output of the flow."""
        self._output = self._convert_to_value_template(output_data)
        return self

    def build(self) -> Flow:
        """Build the Flow object."""
        if self._output is None:
            raise ValueError(
                "Flow output must be set before building. Use set_output() to specify "
                "the flow output."
            )

        return Flow(
            name=self.name,
            description=self.description,
            version=self.version,
            inputSchema=self.input_schema,
            outputSchema=self.output_schema,
            steps=list(self.steps.values()),
            output=self._output,
        )

    def _convert_to_value_template(self, data: Valuable) -> ValueTemplate | None:
        """Convert arbitrary data to ValueTemplate."""
        return Value._convert_to_value_template(data)

    def _convert_reference_to_expr(
        self, ref: StepReference | WorkflowInput
    ) -> Reference:
        """Convert a reference to a Reference."""
        return Value._convert_reference_to_expr(ref)

    def get_references(self) -> list[StepReference | WorkflowInput]:
        """Extract all references used in the current flow."""
        references = []

        # Get references from each step
        for step in self.steps.values():
            references.extend(self._get_step_references(step))

        # Get references from flow output
        if self._output:
            references.extend(self.get_value_template_references(self._output))

        return references

    def _get_step_references(self, step: Step) -> list[StepReference | WorkflowInput]:
        """Extract all references used in a step."""
        references = []

        # Get references from step input
        if step.input:
            references.extend(self.get_value_template_references(step.input))

        # Get references from skipIf condition
        if step.skipIf:
            references.extend(self.get_expr_references(step.skipIf))

        # Get references from onError default value
        if (
            step.onError
            and hasattr(step.onError, "defaultValue")
            and step.onError.defaultValue
        ):
            references.extend(
                self.get_value_template_references(step.onError.defaultValue)
            )

        return references

    def get_value_template_references(
        self, value_template: ValueTemplate
    ) -> list[StepReference | WorkflowInput]:
        """Extract all references from a ValueTemplate."""
        references: list[StepReference | WorkflowInput] = []

        if isinstance(value_template, Reference):
            # This is a $from expression
            references.extend(self.get_expr_references(value_template))
        elif isinstance(value_template, EscapedLiteral):
            # This is a $literal expression - check if it contains nested references
            if isinstance(value_template.field_literal, dict | list):
                references.extend(
                    self.get_value_template_references(value_template.field_literal)
                )
        elif isinstance(value_template, dict):
            # Recursively process dictionary values
            for v in value_template.values():
                references.extend(self.get_value_template_references(v))
        elif isinstance(value_template, list):
            # Recursively process list items
            for item in value_template:
                references.extend(self.get_value_template_references(item))
        # For primitive types (str, int, float, bool, None), no references

        return references

    def get_expr_references(
        self, expr: Reference | EscapedLiteral | GeneratedValue
    ) -> list[StepReference | WorkflowInput]:
        """Extract references from an Expr."""
        references: list[StepReference | WorkflowInput] = []

        if isinstance(expr, Reference):
            # This is a $from expression
            base_ref = expr.field_from

            if isinstance(base_ref, WorkflowReference):
                # Reference to workflow input
                json_path = JsonPath()
                if expr.path is not None and expr.path != "$":
                    json_path.fragments = [expr.path]
                references.append(WorkflowInput(json_path, expr.onSkip))
            elif isinstance(base_ref, GeneratedStepReference):
                # Reference to step output
                json_path = JsonPath()
                if expr.path is not None and expr.path != "$":
                    json_path.fragments = [expr.path]
                references.append(StepReference(base_ref.step, json_path, expr.onSkip))
        elif isinstance(expr, EscapedLiteral):
            # This is a $literal expression - check if it contains nested references
            if isinstance(expr.field_literal, dict | list):
                references.extend(
                    self.get_value_template_references(expr.field_literal)
                )
        # For plain values, no references

        return references
