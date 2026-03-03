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

"""Builder for creating ValueExpr dicts with a clean API.

This module provides a convenient builder class for creating Stepflow value expressions
as plain JSON-compatible dicts, matching the wire format expected by the Stepflow
engine.

Example:
    >>> from stepflow_py.worker.expressions import ValueExpr
    >>> from stepflow_py.api.models import Step
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


class ValueExpr:
    """Builder for creating value expression dicts with a clean API.

    Each factory method returns a plain dict in the wire format expected by the
    Stepflow engine. See https://stepflow.org/docs/flows/expressions for details.
    """

    @staticmethod
    def step(step_id: str, path: str = "") -> dict:
        """Create a step output reference expression.

        Args:
            step_id: The ID of the step to reference
            path: Optional JSONPath expression to access nested fields

        Returns:
            A dict like {"$step": "id"} or {"$step": "id", "path": "..."}

        Example:
            >>> ValueExpr.step("my_step")
            {"$step": "my_step"}
            >>> ValueExpr.step("my_step", "result.data")
            {"$step": "my_step", "path": "result.data"}
        """
        result: dict = {"$step": step_id}
        if path:
            result["path"] = path
        return result

    @staticmethod
    def input(path: str = "") -> dict:
        """Create a workflow input reference expression.

        Args:
            path: JSONPath expression to access workflow input fields

        Returns:
            A dict like {"$input": "path"}

        Example:
            >>> ValueExpr.input("")
            {"$input": ""}
            >>> ValueExpr.input("user.name")
            {"$input": "user.name"}
        """
        return {"$input": path}

    @staticmethod
    def variable(name: str, path: str = "", default: Any = None) -> dict:
        """Create a variable reference expression.

        Args:
            name: The name of the variable
            path: Optional JSONPath expression to access nested variable fields
            default: Optional default value if variable is not defined

        Returns:
            A dict like ``{"$variable": "name"}`` or
            ``{"$variable": "$.name.sub.path", "default": ...}``

        Example:
            >>> ValueExpr.variable("api_key")
            {"$variable": "api_key"}
            >>> ValueExpr.variable("timeout", default=ValueExpr.literal(30))
            {"$variable": "timeout", "default": {"$literal": 30}}
        """
        variable_path = f"$.{name}.{path}" if path else name
        result: dict = {"$variable": variable_path}
        if default is not None:
            result["default"] = default
        return result

    @staticmethod
    def literal(value: Any) -> dict:
        """Create a literal value expression (escaped from expression evaluation).

        Use this when the value contains $-prefixed keys that should NOT be
        interpreted as expression references.

        Args:
            value: Any JSON-serializable value

        Returns:
            A dict like {"$literal": value}

        Example:
            >>> ValueExpr.literal({"key": "value"})
            {"$literal": {"key": "value"}}
            >>> ValueExpr.literal([1, 2, 3])
            {"$literal": [1, 2, 3]}
        """
        return {"$literal": value}

    @staticmethod
    def if_(condition: Any, then: Any, else_: Any = None) -> dict:
        """Create a conditional expression.

        Args:
            condition: Expression that evaluates to a boolean
            then: Value to use if condition is truthy
            else_: Optional value to use if condition is falsy (defaults to null)

        Returns:
            A dict like ``{"$if": cond, "then": expr}`` or
            ``{"$if": cond, "then": expr, "else": expr}``

        Example:
            >>> ValueExpr.if_(
            ...     condition=ValueExpr.variable("debug_mode"),
            ...     then=ValueExpr.literal({"log_level": "debug"}),
            ...     else_=ValueExpr.literal({"log_level": "info"}),
            ... )
        """
        result: dict = {"$if": condition, "then": then}
        if else_ is not None:
            result["else"] = else_
        return result

    @staticmethod
    def coalesce(*values: Any) -> dict:
        """Create a coalesce expression that returns the first non-null value.

        Args:
            *values: One or more expressions to evaluate in order

        Returns:
            A dict like {"$coalesce": [expr, ...]}

        Example:
            >>> ValueExpr.coalesce(
            ...     ValueExpr.variable("user_config"),
            ...     ValueExpr.variable("default_config"),
            ...     ValueExpr.literal({"timeout": 30}),
            ... )
        """
        return {"$coalesce": list(values)}
