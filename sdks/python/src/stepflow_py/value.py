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

"""Value API for creating workflow values and references."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .generated_protocol import (
    EscapedLiteral,
    Reference,
    SkipAction,
    ValueTemplate,
    WorkflowRef,
    WorkflowReference,
)
from .generated_protocol import (
    StepReference as GeneratedStepReference,
)


class JsonPath:
    """Base class for handling JSON Path syntax across all reference types."""

    def __init__(self):
        """Initialize with an optional initial path."""
        self.fragments = ["$"]

    def push_field(self, name: str) -> JsonPath:
        """Add a field access to the path (e.g., .fieldName). Mutates this instance."""
        self.fragments.append(f".{name}")
        return self

    def push_index(self, key: str | int) -> JsonPath:
        """
        Add an index access to the path (e.g., [0] or ["key"]).

        Mutates this instance.
        """
        if isinstance(key, int):
            self.fragments.append(f"[{key}]")
        else:
            self.fragments.append(f'["{key}"]')
        return self

    def with_field(self, name: str) -> JsonPath:
        path = self.copy()
        path.push_field(name)
        return path

    def with_index(self, key: str | int) -> JsonPath:
        path = self.copy()
        path.push_index(key)
        return path

    def copy(self) -> JsonPath:
        """Create a copy of this JsonPath instance."""
        new_path = JsonPath()
        new_path.fragments = self.fragments.copy()
        return new_path

    def __str__(self) -> str:
        """Return the path string, defaulting to '$' for root."""
        return "".join(self.fragments)

    def __repr__(self) -> str:
        return f"JsonPath({self.fragments!r})"


@dataclass
class StepReference:
    """A reference to a step's output or a field within it."""

    step_id: str
    path: JsonPath = field(default_factory=JsonPath)
    on_skip: SkipAction | None = None

    def __getitem__(self, key: str | int) -> StepReference:
        """Create a nested reference using index access."""
        new_path = self.path.with_index(key)
        return StepReference(self.step_id, new_path, self.on_skip)

    def __getattr__(self, name: str) -> StepReference:
        """Create a nested reference to a field."""
        new_path = self.path.with_field(name)
        return StepReference(self.step_id, new_path, self.on_skip)

    def with_on_skip(self, on_skip: SkipAction) -> StepReference:
        """Create a copy of this reference with the specified onSkip action."""
        return StepReference(self.step_id, self.path, on_skip)


@dataclass
class WorkflowInput:
    """Reference to workflow input."""

    path: JsonPath = field(default_factory=JsonPath)
    on_skip: SkipAction | None = None

    def __getitem__(self, key: str | int) -> WorkflowInput:
        """Create a reference to a specific path in the workflow input."""
        new_path = self.path.with_index(key)
        return WorkflowInput(new_path, self.on_skip)

    def __getattr__(self, name: str) -> WorkflowInput:
        """Create a reference to a field in the workflow input."""
        new_path = self.path.with_field(name)
        return WorkflowInput(new_path, self.on_skip)

    def with_on_skip(self, on_skip: SkipAction) -> WorkflowInput:
        """Create a copy of this reference with the specified onSkip action."""
        return WorkflowInput(self.path, on_skip)


class Value:
    """A value that can be used in workflow definitions.

    This class provides a unified interface for creating values that can be:
    - Literal values (using Value.literal() or Value())
    - References to steps (using Value.step())
    - References to workflow input (using Value.input())
    - Expressions using $literal and $from syntax
    """

    def __init__(self, value: Valuable):
        """Create a Value from a Valuable.

        Args:
            value: The value to wrap. Can be:
                - A literal value (str, int, float, bool, None, dict, list)
                - A StepReference or WorkflowInput
                - An EscapedLiteral
                - Another Value (will be unwrapped)
        """
        self._value = self._unwrap_value(value)

    def _unwrap_value(self, value: Valuable) -> Any:
        """Unwrap a Value if it's wrapped, otherwise return as-is."""
        if isinstance(value, Value):
            return value._value
        return value

    @staticmethod
    def literal(value: Any) -> Value:
        """Create a literal value that won't be expanded as a reference.

        This is equivalent to using $literal in the workflow definition.
        """
        return Value(EscapedLiteral(field_literal=value))

    @staticmethod
    def step(
        step_id: str, path: str | None = None, on_skip: SkipAction | None = None
    ) -> Value:
        """Create a reference to a step's output.

        This is equivalent to using $from with a step reference.
        """
        json_path = JsonPath()
        if path is not None and path != "$":
            json_path.fragments = [path]
        return Value(StepReference(step_id, json_path, on_skip))

    @staticmethod
    def input(path: str | None = None, on_skip: SkipAction | None = None) -> Value:
        """Create a reference to workflow input.

        This is equivalent to using $from with a workflow input reference.
        """
        json_path = JsonPath()
        if path is not None and path != "$":
            json_path.fragments = [path]
        return Value(WorkflowInput(json_path, on_skip))

    def to_value_template(self) -> ValueTemplate:
        """Convert this Value to a ValueTemplate for use in flow definitions."""
        return Value._convert_to_value_template(self._value)

    @staticmethod
    def _convert_to_value_template(data: Any) -> ValueTemplate | None:
        """Convert arbitrary data to ValueTemplate."""
        if data is None:
            return None

        if isinstance(data, Value):
            return Value._convert_to_value_template(data._value)

        if isinstance(data, StepReference | WorkflowInput):
            return Value._convert_reference_to_expr(data)

        if isinstance(data, EscapedLiteral):
            return data

        if isinstance(data, dict):
            converted = {}
            for key, value in data.items():
                converted[key] = Value._convert_to_value_template(value)
            return converted

        if isinstance(data, list):
            return [Value._convert_to_value_template(item) for item in data]

        # For primitive types (str, int, float, bool), return as-is
        return data

    @staticmethod
    def _convert_reference_to_expr(
        ref: StepReference | WorkflowInput,
    ) -> Reference:
        """Convert a reference to a Reference."""
        base_ref: WorkflowReference | GeneratedStepReference
        if isinstance(ref, StepReference):
            base_ref = GeneratedStepReference(step=ref.step_id)
            path_str = str(ref.path) if str(ref.path) != "$" else None
            return Reference(field_from=base_ref, path=path_str, onSkip=ref.on_skip)
        elif isinstance(ref, WorkflowInput):
            base_ref = WorkflowReference(workflow=WorkflowRef.input)
            path_str = str(ref.path) if str(ref.path) != "$" else None
            return Reference(field_from=base_ref, path=path_str, onSkip=ref.on_skip)
        else:
            raise ValueError(f"Unknown reference type: {type(ref)}")

    def __getitem__(self, key: str | int) -> Value:
        """
        Create a nested reference to the given key.

        Raises an error if this is not a reference.
        """
        if isinstance(self._value, StepReference | WorkflowInput):
            return Value(self._value[key])
        else:
            raise TypeError(f"Cannot index into {type(self._value).__name__}")

    def __getattr__(self, name: str) -> Value:
        """
        Create a nested reference to the given name.

        Raises an error if this is not a reference.
        """
        if isinstance(self._value, StepReference | WorkflowInput):
            return Value(getattr(self._value, name))
        else:
            raise AttributeError(
                f"'{type(self._value).__name__}' object has no attribute '{name}'"
            )

    def with_on_skip(self, on_skip: SkipAction) -> Value:
        """Create a copy of this Value with the specified onSkip action (only for
        references)."""
        if isinstance(self._value, StepReference | WorkflowInput):
            return Value(self._value.with_on_skip(on_skip))
        else:
            raise TypeError(f"Cannot set onSkip on {type(self._value).__name__}")


class WorkflowInputValue(Value):
    """Special Value class for workflow input that supports attribute access."""

    def __init__(self):
        super().__init__(WorkflowInput())

    def __getitem__(self, key: str | int) -> Value:
        """Create a nested reference to workflow input."""
        return Value(self._value[key])

    def __getattr__(self, name: str) -> Value:
        """Create a nested reference to workflow input field."""
        return Value(getattr(self._value, name))


# Type alias for things that can be converted to JSON values
Valuable = (
    Value
    | StepReference
    | WorkflowInput
    | EscapedLiteral
    | str
    | int
    | float
    | bool
    | None
    | dict[str, "Valuable"]
    | list["Valuable"]
)
