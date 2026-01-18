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

"""Value API for creating workflow values and references."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, cast

from stepflow_py.api.models import (
    InputRef,
    LiteralExpr,
    PrimitiveValue,
    StepRef,
    ValueExpr,  # Used in variable() method
    VariableRef,
)


class JsonPath:
    """Base class for handling JSON Path syntax across all reference types."""

    def __init__(self):
        """Initialize with an optional initial path."""
        self.fragments = ["$"]

    def push_field(self, *names: str) -> JsonPath:
        """Add field access(es) to the path (e.g., .fieldName). Mutates this instance.

        Args:
            *names: One or more field names to add

        Returns:
            Self for method chaining
        """
        if not names:
            raise ValueError("At least one field name is required")
        for name in names:
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

    def with_field(self, *names: str) -> JsonPath:
        """Create a new JsonPath with additional field access(es).

        Args:
            *names: One or more field names to add

        Returns:
            New JsonPath with fields added
        """
        if not names:
            raise ValueError("At least one field name is required")
        path = self.copy()
        path.push_field(*names)
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

    def __getitem__(self, key: str | int) -> StepReference:
        """Create a nested reference using index access."""
        new_path = self.path.with_index(key)
        return StepReference(self.step_id, new_path)

    def __getattr__(self, name: str) -> StepReference:
        """Create a nested reference to a field."""
        new_path = self.path.with_field(name)
        return StepReference(self.step_id, new_path)


@dataclass
class WorkflowInput:
    """Reference to workflow input."""

    path: JsonPath = field(default_factory=JsonPath)

    def __getitem__(self, key: str | int) -> WorkflowInput:
        """Create a reference to a specific path in the workflow input."""
        new_path = self.path.with_index(key)
        return WorkflowInput(new_path)

    def __getattr__(self, name: str) -> WorkflowInput:
        """Create a reference to a field in the workflow input."""
        new_path = self.path.with_field(name)
        return WorkflowInput(new_path)


class InputPathBuilder:
    """Helper class for building workflow input references with method chaining."""

    def __init__(self):
        self.path = JsonPath()

    def __call__(self, path: str | None = None) -> Value:
        """Make InputPathBuilder callable to maintain API compatibility.

        This allows Value.input() to work returning a WorkflowInput reference.

        Args:
            path: Optional path string

        Returns:
            Value containing WorkflowInput reference with proper path handling
        """
        json_path = JsonPath()
        if path is not None and path != "$":
            json_path.fragments = [path]
        return Value(WorkflowInput(json_path))

    def add_path(self, *segments: str) -> Value:
        """Add path segments and return a Value with workflow input reference.

        Args:
            *segments: Path segments to add (e.g., "message", "config", "temperature")

        Returns:
            Value with WorkflowInput reference

        Example:
            Value.input.add_path("message")  # $.message
            Value.input.add_path("config", "temperature")  # $.config.temperature
        """
        path = self.path.with_field(*segments)
        return Value(WorkflowInput(path))


class NoDefault:
    pass


NO_DEFAULT = NoDefault()


class Value:
    """A value that can be used in workflow definitions.

    This class provides a unified interface for creating values that can be:
    - Literal values (using Value.literal() or Value())
    - References to steps (using Value.step())
    - References to workflow input (using Value.input())
    - Expressions using $literal and $from syntax
    """

    # Class attribute for input path builder (set at module level)
    input: InputPathBuilder

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
    def variable(name: str, default: Any | NoDefault = NO_DEFAULT) -> Value:
        """Create a workflow variable reference.

        Args:
            name: The name of the variable
            default: Optional default value if the variable is not set

        Returns:
            Value representing the variable reference
        """
        # Create VariableRef with $variable field
        # Note: Using Python name 'variable' instead of alias '$variable'
        # Works at runtime due to populate_by_name=True in model config
        if default is NO_DEFAULT:
            return Value(VariableRef(variable=name))
        else:
            # Convert default to ValueExpr
            # First convert to a value expression type (LiteralExpr, etc.)
            # Cast needed since mypy can't narrow out NoDefault in else branch
            default_expr = Value._convert_to_value_expr(cast("Valuable", default))
            return Value(VariableRef(variable=name, default=default_expr))

    @staticmethod
    def literal(value: Any) -> Value:
        """Create a literal value that won't be expanded as a reference.

        This is equivalent to using $literal in the workflow definition.
        """
        return Value(LiteralExpr(literal=value))

    @staticmethod
    def step(step_id: str, path: str | None = None) -> Value:
        """Create a reference to a step's output.

        This is equivalent to using $from with a step reference.
        """
        json_path = JsonPath()
        if path is not None and path != "$":
            json_path.fragments = [path]
        return Value(StepReference(step_id, json_path))

    def to_value_expr(self) -> ValueExpr:
        """Convert this Value to a ValueExpr for use in flow definitions."""
        return Value._convert_to_value_expr(self._value)

    @staticmethod
    def _convert_to_value_expr(data: Valuable) -> ValueExpr:
        """Convert arbitrary data to a value expression type.

        Returns primitives, StepRef, InputRef, VariableRef, LiteralExpr,
        or nested dict/list structures containing these types.
        """
        # None needs LiteralExpr since PrimitiveValue doesn't support it
        if data is None:
            return ValueExpr(LiteralExpr(literal=None))

        # For primitive types (str, int, float, bool), wrap in PrimitiveValue
        if isinstance(data, bool | float | str | int):
            return ValueExpr(PrimitiveValue(data))

        if isinstance(data, Value):
            return Value._convert_to_value_expr(data._value)
        # If already a ValueExpr, return as-is to avoid double-wrapping
        if isinstance(data, ValueExpr):
            return data
        if isinstance(data, StepReference):
            path_str = str(data.path) if str(data.path) != "$" else None
            return ValueExpr(StepRef(step=data.step_id, path=path_str))
        if isinstance(data, WorkflowInput):
            path_str = str(data.path) if str(data.path) != "$" else "$"
            return ValueExpr(InputRef(input=path_str))
        if isinstance(data, StepRef | InputRef | VariableRef | LiteralExpr):
            return ValueExpr(data)
        if isinstance(data, dict):
            converted: dict[str, ValueExpr] = {}
            for key, val in data.items():
                converted[key] = Value._convert_to_value_expr(val)
            return ValueExpr(converted)
        if isinstance(data, list):
            return ValueExpr([Value._convert_to_value_expr(item) for item in data])

        raise ValueError(f"Unsupported value: {data}")

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
    | StepRef
    | InputRef
    | VariableRef
    | LiteralExpr
    | str
    | int
    | float
    | bool
    | None
    | dict[str, "Valuable"]
    | list["Valuable"]
)

# Create a singleton input path builder for the convenient API
# This allows using Value.input.add_path("message") syntax
Value.input = InputPathBuilder()
