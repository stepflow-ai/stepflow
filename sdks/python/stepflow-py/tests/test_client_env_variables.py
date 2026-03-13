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

"""Tests for client-side environment variable population."""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from stepflow_py.client import _parse_env_value


def test_parse_env_value_string():
    """Plain strings are returned as-is."""
    assert _parse_env_value("hello") == "hello"
    assert _parse_env_value("sk-abc123") == "sk-abc123"


def test_parse_env_value_json_number():
    """Numeric JSON values are parsed."""
    assert _parse_env_value("42") == 42
    assert _parse_env_value("3.14") == 3.14


def test_parse_env_value_json_boolean():
    """Boolean JSON values are parsed."""
    assert _parse_env_value("true") is True
    assert _parse_env_value("false") is False


def test_parse_env_value_json_null():
    """JSON null is parsed."""
    assert _parse_env_value("null") is None


def test_parse_env_value_json_object():
    """JSON objects are parsed."""
    assert _parse_env_value('{"key": "value"}') == {"key": "value"}


def test_parse_env_value_empty_string():
    """Empty string is returned as-is (not parsed as JSON)."""
    assert _parse_env_value("") == ""


def _make_flow_variables_response(env_vars: dict[str, str]):
    """Create a mock GetFlowVariablesResponse proto message."""
    from stepflow_py.proto import flows_pb2

    response = flows_pb2.GetFlowVariablesResponse(flow_id="test-flow-id")
    for var_name, env_var_name in env_vars.items():
        var_def = flows_pb2.FlowVariable(env_var=env_var_name)
        response.variables[var_name].CopyFrom(var_def)
    return response


def _make_client_with_mock_stubs():
    """Create a StepflowClient with a mocked gRPC channel and stubs."""
    from stepflow_py.client import StepflowClient

    client = StepflowClient.__new__(StepflowClient)
    client._channel = MagicMock()
    client._flows_stub = MagicMock()
    client._runs_stub = MagicMock()
    client._health_stub = MagicMock()
    return client


@pytest.mark.asyncio
async def test_merge_env_variables_populates_from_env():
    """Test that _merge_env_variables reads env vars based on annotations."""
    mock_response = _make_flow_variables_response(
        {
            "api_key": "OPENAI_API_KEY",
            "db_url": "DATABASE_URL",
        }
    )

    client = _make_client_with_mock_stubs()
    client._flows_stub.GetFlowVariables = AsyncMock(return_value=mock_response)

    with patch.dict(
        "os.environ",
        {"OPENAI_API_KEY": "sk-test-123", "DATABASE_URL": "postgres://localhost"},
    ):
        result = await client._merge_env_variables("test-flow-id", None)

    assert result == {
        "api_key": "sk-test-123",
        "db_url": "postgres://localhost",
    }


@pytest.mark.asyncio
async def test_merge_env_variables_explicit_wins():
    """Explicit variables take priority over environment values."""
    mock_response = _make_flow_variables_response({"api_key": "OPENAI_API_KEY"})

    client = _make_client_with_mock_stubs()
    client._flows_stub.GetFlowVariables = AsyncMock(return_value=mock_response)

    with patch.dict("os.environ", {"OPENAI_API_KEY": "sk-from-env"}):
        result = await client._merge_env_variables(
            "test-flow-id", {"api_key": "sk-explicit"}
        )

    assert result == {"api_key": "sk-explicit"}


@pytest.mark.asyncio
async def test_merge_env_variables_missing_env_var():
    """Missing env vars are skipped."""
    mock_response = _make_flow_variables_response(
        {
            "api_key": "OPENAI_API_KEY",
            "missing": "NONEXISTENT_VAR",
        }
    )

    client = _make_client_with_mock_stubs()
    client._flows_stub.GetFlowVariables = AsyncMock(return_value=mock_response)

    with patch.dict("os.environ", {"OPENAI_API_KEY": "sk-test"}, clear=False):
        # Make sure NONEXISTENT_VAR is not set
        import os

        os.environ.pop("NONEXISTENT_VAR", None)
        result = await client._merge_env_variables("test-flow-id", None)

    assert result == {"api_key": "sk-test"}


@pytest.mark.asyncio
async def test_merge_env_variables_no_annotations():
    """Returns explicit_variables when flow has no env_var annotations."""
    mock_response = _make_flow_variables_response({})

    client = _make_client_with_mock_stubs()
    client._flows_stub.GetFlowVariables = AsyncMock(return_value=mock_response)

    result = await client._merge_env_variables("test-flow-id", {"key": "value"})
    assert result == {"key": "value"}
