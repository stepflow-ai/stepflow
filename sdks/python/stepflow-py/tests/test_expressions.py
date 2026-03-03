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

"""Tests for ValueExpr builder class.

All ValueExpr factory methods return plain dicts in the wire format expected
by the Stepflow engine, with no wrapper objects.
"""

import json

from stepflow_py.worker import ValueExpr


def test_step_reference():
    """Test creating step output references returns plain dicts."""
    # Simple step reference
    expr = ValueExpr.step("my_step")
    assert expr == {"$step": "my_step"}

    # Step reference with path
    expr_with_path = ValueExpr.step("my_step", "result.data")
    assert expr_with_path == {"$step": "my_step", "path": "result.data"}


def test_step_reference_no_path_omitted():
    """Test that empty path is omitted from step reference."""
    expr = ValueExpr.step("my_step", "")
    assert expr == {"$step": "my_step"}
    assert "path" not in expr


def test_input_reference():
    """Test creating workflow input references returns plain dicts."""
    # Empty path (root)
    expr = ValueExpr.input("")
    assert expr == {"$input": ""}

    # Nested field reference
    expr_nested = ValueExpr.input("user.name")
    assert expr_nested == {"$input": "user.name"}

    # JSONPath reference
    expr_jsonpath = ValueExpr.input("$.user.name")
    assert expr_jsonpath == {"$input": "$.user.name"}


def test_variable_reference():
    """Test creating variable references returns plain dicts."""
    # Simple variable
    expr = ValueExpr.variable("api_key")
    assert expr == {"$variable": "api_key"}

    # Variable with nested path
    expr_nested = ValueExpr.variable("config", path="api.timeout")
    assert expr_nested == {"$variable": "config.api.timeout"}

    # Variable with default
    default_val = ValueExpr.literal("default-key")
    expr_with_default = ValueExpr.variable("api_key", default=default_val)
    assert expr_with_default == {
        "$variable": "api_key",
        "default": {"$literal": "default-key"},
    }


def test_literal():
    """Test creating literal values returns plain dicts."""
    # String literal
    expr_str = ValueExpr.literal("hello")
    assert expr_str == {"$literal": "hello"}

    # Number literal
    expr_num = ValueExpr.literal(42)
    assert expr_num == {"$literal": 42}

    # Object literal
    expr_obj = ValueExpr.literal({"key": "value"})
    assert expr_obj == {"$literal": {"key": "value"}}

    # List literal
    expr_list = ValueExpr.literal([1, 2, 3])
    assert expr_list == {"$literal": [1, 2, 3]}


def test_conditional():
    """Test creating conditional expressions returns plain dicts."""
    condition = ValueExpr.variable("debug_mode")
    then_val = ValueExpr.literal({"verbose": True})
    else_val = ValueExpr.literal({"verbose": False})

    # With else clause
    expr = ValueExpr.if_(condition, then_val, else_val)
    assert expr == {
        "$if": {"$variable": "debug_mode"},
        "then": {"$literal": {"verbose": True}},
        "else": {"$literal": {"verbose": False}},
    }

    # Without else clause
    expr_no_else = ValueExpr.if_(condition, then_val)
    assert expr_no_else == {
        "$if": {"$variable": "debug_mode"},
        "then": {"$literal": {"verbose": True}},
    }
    assert "else" not in expr_no_else


def test_coalesce():
    """Test creating coalesce expressions returns plain dicts."""
    val1 = ValueExpr.variable("user_config")
    val2 = ValueExpr.variable("default_config")
    val3 = ValueExpr.literal({"timeout": 30})

    expr = ValueExpr.coalesce(val1, val2, val3)
    assert expr == {
        "$coalesce": [
            {"$variable": "user_config"},
            {"$variable": "default_config"},
            {"$literal": {"timeout": 30}},
        ]
    }


def test_serialization():
    """Test that expressions serialize correctly to JSON."""
    # Step reference
    step_expr = ValueExpr.step("my_step", "result")
    data = json.loads(json.dumps(step_expr))
    assert data["$step"] == "my_step"
    assert data["path"] == "result"

    # Input reference
    input_expr = ValueExpr.input("user.name")
    data = json.loads(json.dumps(input_expr))
    assert data["$input"] == "user.name"

    # Variable reference
    var_expr = ValueExpr.variable("api_key")
    data = json.loads(json.dumps(var_expr))
    assert data["$variable"] == "api_key"

    # Literal
    literal_expr = ValueExpr.literal({"key": "value"})
    data = json.loads(json.dumps(literal_expr))
    assert "$literal" in data
    assert data["$literal"] == {"key": "value"}


def test_complex_expression():
    """Test creating and serializing a complex nested expression."""
    complex_expr = ValueExpr.if_(
        condition=ValueExpr.variable("debug_mode"),
        then=ValueExpr.step("debug_step", "result"),
        else_=ValueExpr.coalesce(
            ValueExpr.step("fallback_step", "output"),
            ValueExpr.literal({"status": "default"}),
        ),
    )

    # Verify structure
    assert "$if" in complex_expr
    assert complex_expr["$if"] == {"$variable": "debug_mode"}
    assert complex_expr["then"] == {"$step": "debug_step", "path": "result"}
    assert "$coalesce" in complex_expr["else"]

    # Verify serialization works
    data = json.loads(json.dumps(complex_expr))
    assert "$if" in data
    assert data["$if"]["$variable"] == "debug_mode"
    assert data["then"]["$step"] == "debug_step"
    assert "$coalesce" in data["else"]
