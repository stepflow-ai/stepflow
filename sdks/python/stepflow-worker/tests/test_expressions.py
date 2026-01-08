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

import msgspec

from stepflow_worker import ValueExpr
from stepflow_worker.generated_flow import (
    Coalesce,
    If,
    InputRef,
    LiteralExpr,
    StepRef,
    VariableRef,
)


def test_step_reference():
    """Test creating step output references."""
    # Simple step reference
    expr = ValueExpr.step("my_step")
    assert isinstance(expr, StepRef)
    assert expr.field_step == "my_step"
    assert expr.path is None

    # Step reference with path
    expr_with_path = ValueExpr.step("my_step", "result.data")
    assert isinstance(expr_with_path, StepRef)
    assert expr_with_path.field_step == "my_step"
    assert expr_with_path.path == "result.data"


def test_input_reference():
    """Test creating workflow input references."""
    # Empty path (root)
    expr = ValueExpr.input("")
    assert isinstance(expr, InputRef)
    assert expr.field_input == ""

    # Nested field reference
    expr_nested = ValueExpr.input("user.name")
    assert isinstance(expr_nested, InputRef)
    assert expr_nested.field_input == "user.name"


def test_variable_reference():
    """Test creating variable references."""
    # Simple variable
    expr = ValueExpr.variable("api_key")
    assert isinstance(expr, VariableRef)
    assert expr.field_variable == "api_key"
    assert expr.default is None

    # Variable with default
    default_val = ValueExpr.literal("default-key")
    expr_with_default = ValueExpr.variable("api_key", default=default_val)
    assert isinstance(expr_with_default, VariableRef)
    assert expr_with_default.field_variable == "api_key"
    assert expr_with_default.default == default_val

    # Variable with nested path
    expr_nested = ValueExpr.variable("config", path="api.timeout")
    assert isinstance(expr_nested, VariableRef)
    assert expr_nested.field_variable == "config.api.timeout"


def test_literal():
    """Test creating literal values."""
    # String literal
    expr_str = ValueExpr.literal("hello")
    assert isinstance(expr_str, LiteralExpr)
    assert expr_str.field_literal == "hello"

    # Number literal
    expr_num = ValueExpr.literal(42)
    assert isinstance(expr_num, LiteralExpr)
    assert expr_num.field_literal == 42

    # Object literal
    expr_obj = ValueExpr.literal({"key": "value"})
    assert isinstance(expr_obj, LiteralExpr)
    assert expr_obj.field_literal == {"key": "value"}


def test_conditional():
    """Test creating conditional expressions."""
    condition = ValueExpr.variable("debug_mode")
    then_val = ValueExpr.literal({"verbose": True})
    else_val = ValueExpr.literal({"verbose": False})

    # With else clause
    expr = ValueExpr.if_(condition, then_val, else_val)
    assert isinstance(expr, If)
    assert expr.field_if == condition
    assert expr.then == then_val
    assert expr.else_ == else_val

    # Without else clause
    expr_no_else = ValueExpr.if_(condition, then_val)
    assert isinstance(expr_no_else, If)
    assert expr_no_else.field_if == condition
    assert expr_no_else.then == then_val
    assert expr_no_else.else_ is None


def test_coalesce():
    """Test creating coalesce expressions."""
    val1 = ValueExpr.variable("user_config")
    val2 = ValueExpr.variable("default_config")
    val3 = ValueExpr.literal({"timeout": 30})

    expr = ValueExpr.coalesce(val1, val2, val3)
    assert isinstance(expr, Coalesce)
    assert len(expr.field_coalesce) == 3
    assert expr.field_coalesce[0] == val1
    assert expr.field_coalesce[1] == val2
    assert expr.field_coalesce[2] == val3


def test_serialization():
    """Test that expressions serialize correctly."""
    # Step reference
    step_expr = ValueExpr.step("my_step", "result")
    serialized = msgspec.json.encode(step_expr)
    assert b'"$step":"my_step"' in serialized
    assert b'"path":"result"' in serialized

    # Input reference
    input_expr = ValueExpr.input("user.name")
    serialized = msgspec.json.encode(input_expr)
    assert b'"$input":"user.name"' in serialized

    # Variable reference
    var_expr = ValueExpr.variable("api_key")
    serialized = msgspec.json.encode(var_expr)
    assert b'"$variable":"api_key"' in serialized

    # Literal
    literal_expr = ValueExpr.literal({"key": "value"})
    serialized = msgspec.json.encode(literal_expr)
    assert b'"$literal"' in serialized
    assert b'"key":"value"' in serialized


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

    # Verify structure
    assert isinstance(complex_expr, If)
    assert isinstance(complex_expr.field_if, VariableRef)
    assert isinstance(complex_expr.then, StepRef)
    assert isinstance(complex_expr.else_, Coalesce)

    # Verify serialization works
    serialized = msgspec.json.encode(complex_expr)
    assert b'"$if"' in serialized
    assert b'"$variable":"debug_mode"' in serialized
    assert b'"$step":"debug_step"' in serialized
    assert b'"$coalesce"' in serialized
