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

"""Tests for ValueExpr builder class."""

import json

from stepflow_py.api.models import (
    Coalesce,
    InputRef,
    LiteralExpr,
    ModelIf,
    StepRef,
    VariableRef,
)
from stepflow_py.worker import ValueExpr


def test_step_reference():
    """Test creating step output references."""
    # Simple step reference
    expr = ValueExpr.step("my_step")
    assert isinstance(expr, StepRef)
    assert expr.step == "my_step"
    assert expr.path is None

    # Step reference with path
    expr_with_path = ValueExpr.step("my_step", "result.data")
    assert isinstance(expr_with_path, StepRef)
    assert expr_with_path.step == "my_step"
    assert expr_with_path.path == "result.data"


def test_input_reference():
    """Test creating workflow input references."""
    # Empty path (root)
    expr = ValueExpr.input("")
    assert isinstance(expr, InputRef)
    assert expr.input == ""

    # Nested field reference
    expr_nested = ValueExpr.input("user.name")
    assert isinstance(expr_nested, InputRef)
    assert expr_nested.input == "user.name"


def test_variable_reference():
    """Test creating variable references."""
    # Simple variable
    expr = ValueExpr.variable("api_key")
    assert isinstance(expr, VariableRef)
    assert expr.variable == "api_key"
    assert expr.default is None

    # Variable with default
    default_val = ValueExpr.literal("default-key")
    expr_with_default = ValueExpr.variable("api_key", default=default_val)
    assert isinstance(expr_with_default, VariableRef)
    assert expr_with_default.variable == "api_key"
    # Default is now wrapped in ValueExpr, check the actual_instance
    assert expr_with_default.default.actual_instance == default_val

    # Variable with nested path
    expr_nested = ValueExpr.variable("config", path="api.timeout")
    assert isinstance(expr_nested, VariableRef)
    assert expr_nested.variable == "config.api.timeout"


def test_literal():
    """Test creating literal values."""
    # String literal
    expr_str = ValueExpr.literal("hello")
    assert isinstance(expr_str, LiteralExpr)
    assert expr_str.literal == "hello"

    # Number literal
    expr_num = ValueExpr.literal(42)
    assert isinstance(expr_num, LiteralExpr)
    assert expr_num.literal == 42

    # Object literal
    expr_obj = ValueExpr.literal({"key": "value"})
    assert isinstance(expr_obj, LiteralExpr)
    assert expr_obj.literal == {"key": "value"}


def test_conditional():
    """Test creating conditional expressions."""
    condition = ValueExpr.variable("debug_mode")
    then_val = ValueExpr.literal({"verbose": True})
    else_val = ValueExpr.literal({"verbose": False})

    # With else clause
    expr = ValueExpr.if_(condition, then_val, else_val)
    assert isinstance(expr, ModelIf)
    # Fields are wrapped in ValueExpr, check actual_instance
    assert expr.var_if.actual_instance == condition
    assert expr.then.actual_instance == then_val
    assert expr.var_else.actual_instance == else_val

    # Without else clause
    expr_no_else = ValueExpr.if_(condition, then_val)
    assert isinstance(expr_no_else, ModelIf)
    assert expr_no_else.var_if.actual_instance == condition
    assert expr_no_else.then.actual_instance == then_val
    assert expr_no_else.var_else is None


def test_coalesce():
    """Test creating coalesce expressions."""
    val1 = ValueExpr.variable("user_config")
    val2 = ValueExpr.variable("default_config")
    val3 = ValueExpr.literal({"timeout": 30})

    expr = ValueExpr.coalesce(val1, val2, val3)
    assert isinstance(expr, Coalesce)
    assert len(expr.coalesce) == 3
    # Values are wrapped in ValueExpr, check actual_instance
    assert expr.coalesce[0].actual_instance == val1
    assert expr.coalesce[1].actual_instance == val2
    assert expr.coalesce[2].actual_instance == val3


def test_serialization():
    """Test that expressions serialize correctly."""
    # Step reference
    step_expr = ValueExpr.step("my_step", "result")
    serialized = step_expr.to_json()
    data = json.loads(serialized)
    assert data["$step"] == "my_step"
    assert data["path"] == "result"

    # Input reference
    input_expr = ValueExpr.input("user.name")
    serialized = input_expr.to_json()
    data = json.loads(serialized)
    assert data["$input"] == "user.name"

    # Variable reference
    var_expr = ValueExpr.variable("api_key")
    serialized = var_expr.to_json()
    data = json.loads(serialized)
    assert data["$variable"] == "api_key"

    # Literal
    literal_expr = ValueExpr.literal({"key": "value"})
    serialized = literal_expr.to_json()
    data = json.loads(serialized)
    assert "$literal" in data
    assert data["$literal"] == {"key": "value"}


def test_complex_expression():
    """Test creating and serializing a complex nested expression."""
    # Create a complex expression with nested references
    complex_expr = ValueExpr.if_(
        condition=ValueExpr.variable("debug_mode"),
        then=ValueExpr.step("debug_step", "result"),
        else_=ValueExpr.coalesce(
            ValueExpr.step("fallback_step", "output"),
            ValueExpr.literal({"status": "default"}),
        ),
    )

    # Verify structure - fields are wrapped in ValueExpr
    assert isinstance(complex_expr, ModelIf)
    assert isinstance(complex_expr.var_if.actual_instance, VariableRef)
    assert isinstance(complex_expr.then.actual_instance, StepRef)
    assert isinstance(complex_expr.var_else.actual_instance, Coalesce)

    # Verify serialization works
    serialized = complex_expr.to_json()
    data = json.loads(serialized)
    assert "$if" in data
    assert data["$if"]["$variable"] == "debug_mode"
    assert data["then"]["$step"] == "debug_step"
    assert "$coalesce" in data["else"]
