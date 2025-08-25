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

"""Flow builder for creating Stepflow workflows programmatically."""

from __future__ import annotations

from dataclasses import dataclass, is_dataclass
from typing import Any, assert_never

import msgspec

from .generated_flow import (
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
from .generated_flow import (
    StepReference as GeneratedStepReference,
)
from .generated_flow import (
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
    """Builder for creating Stepflow workflows.

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
        metadata: dict[str, Any] | None = None,
    ):
        self.name = name
        self.description = description
        self.version = version
        self.metadata = metadata or {}
        self.input_schema: Schema | None = None
        self.output_schema: Schema | None = None
        self.steps: dict[str, Step] = {}
        self._step_handles: dict[str, StepHandle] = {}
        self._output: ValueTemplate | None = None
        self._output_fields: dict[str, Valuable] = {}  # For incremental output building

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
            name=flow.name,
            description=flow.description,
            version=flow.version,
            metadata=flow.metadata,
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

    def _ensure_unique_step_id(self, preferred_id: str) -> str:
        """Ensure the step ID is unique by adding a suffix if needed."""
        if preferred_id not in self.steps:
            return preferred_id

        counter = 2
        while f"{preferred_id}_{counter}" in self.steps:
            counter += 1
        return f"{preferred_id}_{counter}"

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

    def set_metadata(self, metadata: dict[str, Any]) -> FlowBuilder:
        """Set the metadata for the flow."""
        self.metadata = metadata
        return self

    def add_step(
        self,
        *,
        id: str,
        component: Component,
        input_data: Any = None,  # Accept any data structure
        input_schema: dict[str, Any] | Schema | None = None,
        output_schema: dict[str, Any] | Schema | None = None,
        skip_if: StepReference | WorkflowInput | Value | None = None,
        on_error: ErrorAction | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandle:
        """Add a step to the flow with automatic ID uniqueness and input conversion.

        Automatically:
        1. Ensures step ID is unique by adding suffix if needed
        2. Converts dataclasses/msgspec structs to JSON
        3. Handles step references properly
        """
        # Ensure step ID is unique
        unique_id = self._ensure_unique_step_id(id)

        # Auto-convert input data
        converted_input = self._auto_convert_input(input_data)

        # Convert input data to ValueTemplate
        input_template = self._convert_to_value_template(converted_input)

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
            id=unique_id,
            component=component,
            input=input_template,
            inputSchema=input_schema_obj,
            outputSchema=output_schema_obj,
            skipIf=skip_if_expr,
            onError=on_error_action,
            metadata=metadata or {},
        )

        self.steps[unique_id] = step

        # Create and store handle
        handle = StepHandle(step, self)
        self._step_handles[unique_id] = handle

        return handle

    def set_output(self, output_data: Valuable) -> FlowBuilder:
        """Set the output of the flow."""
        self._output = self._convert_to_value_template(output_data)
        return self

    def add_output_field(self, key: str, value: Valuable) -> FlowBuilder:
        """Add a field to the output incrementally.

        This allows building up structured outputs field by field instead of
        manually managing all output references.

        Args:
            key: The field name in the output
            value: The value for this field (step reference, input reference, etc.)

        Returns:
            Self for chaining

        Example:
            builder.add_output_field("result", step1.result)
            builder.add_output_field("metadata", step2.result)
            # Creates output: {"result": ..., "metadata": ...}
        """
        self._output_fields[key] = value
        return self

    def _auto_convert_input(self, input_data: Any) -> Valuable:
        """Auto-convert various data structures to Valuable format.

        Args:
            input_data: Data to convert

        Returns:
            Converted data suitable for Valuable
        """
        if input_data is None:
            return None

        # Already a Valuable type - pass through
        if isinstance(
            input_data,
            Value
            | StepReference
            | WorkflowInput
            | EscapedLiteral
            | str
            | int
            | float
            | bool,
        ):
            return input_data

        # Handle dataclasses
        if is_dataclass(input_data):
            # Convert dataclass to dict
            import dataclasses

            assert not isinstance(input_data, type)
            converted = dataclasses.asdict(input_data)
            return converted

        # Handle msgspec structs
        if hasattr(input_data, "__struct_fields__"):  # msgspec struct
            # Convert msgspec struct to dict
            converted = msgspec.structs.asdict(input_data)
            return converted

        # Handle regular dicts and lists - recursively convert any nested structures
        if isinstance(input_data, dict):
            return {k: self._auto_convert_input(v) for k, v in input_data.items()}

        if isinstance(input_data, list):
            return [self._auto_convert_input(item) for item in input_data]

        assert_never(input_data)

    def build(self) -> Flow:
        """Build the Flow object."""
        # Determine output from either explicit output or accumulated fields
        output_to_use = None
        if self._output is not None:
            output_to_use = self._output
        elif self._output_fields:
            # Build output from accumulated fields
            output_to_use = self._convert_to_value_template(self._output_fields)
        else:
            raise ValueError(
                "Flow output must be set before building. Use set_output() or "
                "add_output_field() to specify the flow output."
            )

        return Flow(
            schema_="https://stepflow.org/schemas/v1/flow.json",
            name=self.name,
            description=self.description,
            version=self.version,
            inputSchema=self.input_schema,
            outputSchema=self.output_schema,
            steps=list(self.steps.values()),
            output=output_to_use,
            metadata=self.metadata,
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
