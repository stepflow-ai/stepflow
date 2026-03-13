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

"""Flow builder for creating Stepflow workflows programmatically.

Flows and steps use msgspec Struct types generated from schemas/flow.json.
Serialize with ``msgspec.to_builtins(flow)`` to get a plain dict suitable
for JSON/YAML or the ``store_flow`` API.
"""

from __future__ import annotations

from dataclasses import dataclass, is_dataclass
from typing import Any

import msgspec
from typing_extensions import assert_never

from .generated_flow import (
    ErrorAction,
    Flow,
    OnErrorDefault,
    Step,
)
from .value import (
    JsonPath,
    StepReference,
    Valuable,
    Value,
    WorkflowInput,
)

# Type alias for component (just a string in the API)
Component = str


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
    - Use FlowBuilder.load(flow) to create a builder from an existing Flow or dict

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
        self.input_schema: dict[str, Any] | None = None
        self.output_schema: dict[str, Any] | None = None
        self.variables_schema: dict[str, Any] | None = None
        self.steps: dict[str, Step] = {}
        self._step_handles: dict[str, StepHandle] = {}
        self._output: Any | None = None
        self._output_fields: dict[str, Valuable] = {}  # For incremental output building

    @classmethod
    def load(cls, flow: Flow | dict[str, Any]) -> FlowBuilder:
        """Create a FlowBuilder from an existing Flow or dict.

        This allows you to load an existing flow and analyze or modify it.

        Example:
            builder = FlowBuilder.load(existing_flow)
            references = builder.get_references()
            step_refs = builder.step("step_name").get_references()
        """
        # Convert dict to Flow struct if needed
        if isinstance(flow, dict):
            flow = msgspec.convert(flow, Flow)

        builder = cls(
            name=flow.name if flow.name is not msgspec.UNSET else None,
            description=flow.description
            if flow.description is not msgspec.UNSET
            else None,
            version=flow.version if flow.version is not msgspec.UNSET else None,
            metadata=flow.metadata if flow.metadata is not msgspec.UNSET else None,
        )
        # Extract schemas from the FlowSchema struct
        if flow.schemas is not msgspec.UNSET:
            # FlowSchema is an open struct — convert to dict for inspection
            schema_dict = msgspec.to_builtins(flow.schemas)
            props = (
                schema_dict.get("properties", {})
                if isinstance(schema_dict, dict)
                else {}
            )
            builder.input_schema = props.get("input")
            builder.output_schema = props.get("output")
            builder.variables_schema = props.get("variables")
        if flow.output is not msgspec.UNSET:
            builder._output = flow.output

        # Recreate steps and step handles
        steps = flow.steps if flow.steps is not msgspec.UNSET else []
        for step in steps or []:
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

    def set_input_schema(self, schema: dict[str, Any]) -> FlowBuilder:
        """Set the input schema for the flow."""
        self.input_schema = schema
        return self

    def set_output_schema(self, schema: dict[str, Any]) -> FlowBuilder:
        """Set the output schema for the flow."""
        self.output_schema = schema
        return self

    def set_variables_schema(self, schema: dict[str, Any]) -> FlowBuilder:
        """Set the variables schema for the flow.

        The variables schema defines workflow variables with their types,
        default values, descriptions, and secret annotations.

        Args:
            schema: Variables schema as a dictionary containing JSON schema properties.

        Returns:
            FlowBuilder: Self for method chaining.

        Example:
            builder.set_variables_schema({
                "type": "object",
                "properties": {
                    "api_key": {
                        "type": "string",
                        "is_secret": True,
                        "description": "OpenAI API key"
                    },
                    "temperature": {
                        "type": "number",
                        "default": 0.7,
                        "minimum": 0,
                        "maximum": 2
                    }
                },
                "required": ["api_key"]
            })
        """
        self.variables_schema = schema
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
        on_error: ErrorAction | None = None,
        must_execute: bool | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandle:
        """Add a step to the flow with automatic ID uniqueness and input conversion.

        Automatically:
        1. Ensures step ID is unique by adding suffix if needed
        2. Converts dataclasses/msgspec structs to JSON
        3. Handles step references properly

        Args:
            on_error: Error handling — use OnErrorFail(), OnErrorDefault(), or
                OnErrorRetry() from stepflow_py.worker.
        """
        # Ensure step ID is unique
        unique_id = self._ensure_unique_step_id(id)

        # Auto-convert input data
        converted_input = self._auto_convert_input(input_data)

        # Convert input data to JSON-compatible value
        input_expr = self._convert_to_value_expr(converted_input)

        # Build Step struct
        step = Step(
            id=unique_id,
            component=component,
            input=input_expr,
            onError=on_error,
            mustExecute=must_execute,
            metadata=metadata or {},
        )

        self.steps[unique_id] = step

        # Create and store handle
        handle = StepHandle(step, self)
        self._step_handles[unique_id] = handle

        return handle

    def set_output(self, output_data: Valuable) -> FlowBuilder:
        """Set the output of the flow."""
        self._output = self._convert_to_value_expr(output_data)
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
            Value | StepReference | WorkflowInput | str | int | float | bool,
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

        # Handle Pydantic models
        if hasattr(input_data, "model_dump"):  # pydantic model
            pydantic_dict: dict[str, Any] = input_data.model_dump(
                by_alias=True, exclude_none=True
            )
            return pydantic_dict

        # Handle regular dicts and lists - recursively convert any nested structures
        if isinstance(input_data, dict):
            return {k: self._auto_convert_input(v) for k, v in input_data.items()}

        if isinstance(input_data, list):
            return [self._auto_convert_input(item) for item in input_data]

        assert_never(input_data)

    def build(self) -> Flow:
        """Build the Flow as a msgspec Struct.

        Serialize the result with ``msgspec.to_builtins(flow)`` to get a
        plain dict suitable for JSON, YAML, or the ``store_flow`` API.
        """
        # Determine output from either explicit output or accumulated fields
        output_to_use: Any = msgspec.UNSET
        if self._output is not None:
            output_to_use = self._output
        elif self._output_fields:
            # Build output from accumulated fields
            output_to_use = self._convert_to_value_expr(self._output_fields)
        else:
            raise ValueError(
                "Flow output must be set before building. Use set_output() or "
                "add_output_field() to specify the flow output."
            )

        # Build schemas dict in JSON Schema format if any schemas are set
        schemas: Any = msgspec.UNSET
        if (
            self.input_schema is not None
            or self.output_schema is not None
            or self.variables_schema is not None
        ):
            properties: dict[str, Any] = {}
            if self.input_schema is not None:
                properties["input"] = self.input_schema
            if self.output_schema is not None:
                properties["output"] = self.output_schema
            if self.variables_schema is not None:
                properties["variables"] = self.variables_schema
            schema_dict: dict[str, Any] = {"type": "object"}
            if properties:
                schema_dict["properties"] = properties
            schemas = schema_dict

        return Flow(
            name=self.name,
            description=self.description,
            version=self.version,
            schemas=schemas,
            steps=list(self.steps.values()),
            output=output_to_use,
            metadata=self.metadata if self.metadata else msgspec.UNSET,
        )

    def _convert_to_value_expr(self, data: Valuable) -> Any:
        """Convert arbitrary data to a JSON-compatible value expression."""
        return Value._convert_to_value_expr(data)

    def get_references(self) -> list[StepReference | WorkflowInput]:
        """Extract all references used in the current flow."""
        references = []

        # Get references from each step
        for step in self.steps.values():
            references.extend(self._get_step_references(step))

        # Get references from flow output
        if self._output:
            references.extend(self.get_value_expr_references(self._output))

        return references

    def _get_step_references(self, step: Step) -> list[StepReference | WorkflowInput]:
        """Extract all references used in a step."""
        references = []

        # Get references from step input
        step_input = step.input if step.input is not msgspec.UNSET else None
        if step_input:
            references.extend(self.get_value_expr_references(step_input))

        # Get references from onError default value
        on_error = step.onError if step.onError is not msgspec.UNSET else None
        if on_error and isinstance(on_error, OnErrorDefault):
            default_val = (
                on_error.defaultValue
                if on_error.defaultValue is not msgspec.UNSET
                else None
            )
            if default_val:
                references.extend(self.get_value_expr_references(default_val))

        return references

    def get_value_expr_references(
        self, value_expr: Any
    ) -> list[StepReference | WorkflowInput]:
        """Extract all step/input references from a plain JSON value expression."""
        references: list[StepReference | WorkflowInput] = []

        if isinstance(value_expr, dict):
            if "$step" in value_expr:
                # Step reference: {"$step": "step_id", "path"?: "..."}
                json_path = JsonPath()
                path = value_expr.get("path")
                if path is not None and path != "$":
                    json_path.fragments = [path]
                references.append(StepReference(value_expr["$step"], json_path))
            elif "$input" in value_expr:
                # Input reference: {"$input": "path"}
                json_path = JsonPath()
                input_path = value_expr["$input"]
                if input_path is not None and input_path != "$":
                    json_path.fragments = [input_path]
                references.append(WorkflowInput(json_path))
            elif "$variable" in value_expr:
                # Variable reference - no step/input refs to extract
                pass
            elif "$literal" in value_expr:
                # Escaped literal — opaque leaf, inner value is never evaluated
                pass
            elif "$if" in value_expr:
                # Conditional - recurse into condition, then, else
                references.extend(self.get_value_expr_references(value_expr["$if"]))
                if "then" in value_expr:
                    references.extend(
                        self.get_value_expr_references(value_expr["then"])
                    )
                if "else" in value_expr:
                    references.extend(
                        self.get_value_expr_references(value_expr["else"])
                    )
            elif "$coalesce" in value_expr:
                # Coalesce - recurse into each item
                for item in value_expr.get("$coalesce", []):
                    references.extend(self.get_value_expr_references(item))
            else:
                # Regular dict - recurse into all values
                for v in value_expr.values():
                    references.extend(self.get_value_expr_references(v))
        elif isinstance(value_expr, list):
            for item in value_expr:
                references.extend(self.get_value_expr_references(item))
        # For primitives (str, int, float, bool, None), no references

        return references
