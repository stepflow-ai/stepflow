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

"""Builder for creating ValueExpr instances with a clean API.

This module provides a convenient builder class for creating Stepflow value expressions
without having to directly instantiate the generated ValueExpr types.

Example:
    >>> from stepflow_py.expressions import ValueExpr
    >>> from stepflow_py.generated_flow import Step
    >>>
    >>> # Create a step that references another step's output
    >>> step = Step(
    ...     id="process",
    ...     component="/python/processor",
    ...     input=ValueExpr.step("previous_step", "result.data"),
    ... )
    >>>
    >>> # Create a conditional expression
    >>> conditional = ValueExpr.if_(
    ...     condition=ValueExpr.variable("debug_mode"),
    ...     then=ValueExpr.literal({"verbose": True}),
    ...     else_=ValueExpr.literal({"verbose": False}),
    ... )
"""

from __future__ import annotations

from typing import Any

from .generated_flow import (
    Coalesce,
    If,
    InputRef,
    LiteralExpr,
    StepRef,
    VariableRef,
)
from .generated_flow import (
    ValueExpr as GenValueExpr,
)


class ValueExpr:
    """Builder for creating ValueExpr instances with a clean API.

    This class provides static factory methods for creating value expressions
    using the new syntax with $step, $input, and $variable.
    """

    @staticmethod
    def step(step_id: str, path: str = "") -> GenValueExpr:
        """Create a step output reference expression.

        Args:
            step_id: The ID of the step to reference
            path: Optional JSONPath expression to access nested fields

        Returns:
            A ValueExpr that references the step's output

        Example:
            >>> # Reference entire step output
            >>> ValueExpr.step("my_step")
            >>>
            >>> # Reference nested field
            >>> ValueExpr.step("my_step", "result.data")
        """
        return StepRef(field_step=step_id, path=path if path else None)

    @staticmethod
    def input(path: str) -> GenValueExpr:
        """Create a workflow input reference expression.

        Args:
            path: JSONPath expression to access workflow input fields

        Returns:
            A ValueExpr that references the workflow input

        Example:
            >>> # Reference entire input
            >>> ValueExpr.input("")
            >>>
            >>> # Reference specific field
            >>> ValueExpr.input("user.name")
        """
        return InputRef(field_input=path)

    @staticmethod
    def variable(
        name: str, path: str = "", default: GenValueExpr | None = None
    ) -> GenValueExpr:
        """Create a variable reference expression.

        Args:
            name: The name of the variable
            path: Optional JSONPath expression to access nested variable fields
            default: Optional default value if variable is not defined

        Returns:
            A ValueExpr that references the variable

        Example:
            >>> # Simple variable reference
            >>> ValueExpr.variable("api_key")
            >>>
            >>> # Variable with default value
            >>> ValueExpr.variable("timeout", default=ValueExpr.literal(30))
            >>>
            >>> # Nested variable field
            >>> ValueExpr.variable("config.api.timeout")
        """
        # Combine name and path for the variable field
        variable_path = f"{name}.{path}" if path else name
        return VariableRef(field_variable=variable_path, default=default)

    @staticmethod
    def literal(value: Any) -> GenValueExpr:
        """Create a literal value expression.

        Args:
            value: Any JSON-serializable value

        Returns:
            A ValueExpr that represents a literal value

        Example:
            >>> ValueExpr.literal({"key": "value"})
            >>> ValueExpr.literal([1, 2, 3])
            >>> ValueExpr.literal("hello")
        """
        return LiteralExpr(field_literal=value)

    @staticmethod
    def if_(
        condition: GenValueExpr, then: GenValueExpr, else_: GenValueExpr | None = None
    ) -> GenValueExpr:
        """Create a conditional expression.

        Args:
            condition: Expression that evaluates to a boolean
            then: Value to use if condition is truthy
            else_: Optional value to use if condition is falsy

        Returns:
            A ValueExpr that represents a conditional

        Example:
            >>> ValueExpr.if_(
            ...     condition=ValueExpr.variable("debug_mode"),
            ...     then=ValueExpr.literal({"log_level": "debug"}),
            ...     else_=ValueExpr.literal({"log_level": "info"}),
            ... )
        """
        return If(field_if=condition, then=then, else_=else_)

    @staticmethod
    def coalesce(*values: GenValueExpr) -> GenValueExpr:
        """Create a coalesce expression that returns the first non-null value.

        Args:
            *values: One or more expressions to evaluate in order

        Returns:
            A ValueExpr that represents a coalesce operation

        Example:
            >>> ValueExpr.coalesce(
            ...     ValueExpr.variable("user_config"),
            ...     ValueExpr.variable("default_config"),
            ...     ValueExpr.literal({"timeout": 30}),
            ... )
        """
        return Coalesce(field_coalesce=list(values))
