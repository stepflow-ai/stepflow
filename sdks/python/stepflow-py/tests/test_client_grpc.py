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

"""Tests for gRPC client URL parsing and construction."""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock

import pytest

from stepflow_py.client import (
    StepflowClient,
    _from_proto_value,
    _parse_grpc_target,
    _to_proto_struct,
    _to_proto_value,
)


class TestParseGrpcTarget:
    """Tests for _parse_grpc_target URL parsing."""

    def test_plain_host_port(self):
        target, tls = _parse_grpc_target("localhost:8080")
        assert target == "localhost:8080"
        assert tls is False

    def test_http_url(self):
        target, tls = _parse_grpc_target("http://localhost:8080")
        assert target == "localhost:8080"
        assert tls is False

    def test_https_url(self):
        target, tls = _parse_grpc_target("https://myhost:443")
        assert target == "myhost:443"
        assert tls is True

    def test_http_with_path(self):
        """Strips /api/v1 path from HTTP URLs."""
        target, tls = _parse_grpc_target("http://localhost:8080/api/v1")
        assert target == "localhost:8080"
        assert tls is False

    def test_ip_address(self):
        target, tls = _parse_grpc_target("192.168.1.1:9090")
        assert target == "192.168.1.1:9090"
        assert tls is False

    def test_http_default_port(self):
        target, tls = _parse_grpc_target("http://localhost")
        assert target == "localhost:8080"
        assert tls is False

    def test_https_default_port(self):
        target, tls = _parse_grpc_target("https://myhost")
        assert target == "myhost:443"
        assert tls is True


class TestProtoConversions:
    """Tests for protobuf value conversions."""

    def test_to_proto_value_string(self):
        val = _to_proto_value("hello")
        assert val.string_value == "hello"

    def test_to_proto_value_number(self):
        val = _to_proto_value(42)
        assert val.number_value == 42.0

    def test_to_proto_value_dict(self):
        val = _to_proto_value({"key": "value"})
        assert val.struct_value.fields["key"].string_value == "value"

    def test_to_proto_value_list(self):
        val = _to_proto_value([1, 2, 3])
        assert len(val.list_value.values) == 3

    def test_to_proto_struct(self):
        s = _to_proto_struct({"name": "test", "count": 5})
        assert s.fields["name"].string_value == "test"
        assert s.fields["count"].number_value == 5.0

    def test_from_proto_value_roundtrip(self):
        original = {"key": "value", "num": 42, "nested": {"a": 1}}
        val = _to_proto_value(original)
        result = _from_proto_value(val)
        assert result == original


class TestStepflowClientBuildRequest:
    """Tests for internal request building."""

    def _make_client(self):
        client = StepflowClient.__new__(StepflowClient)
        client._channel = MagicMock()
        client._flows_stub = MagicMock()
        client._runs_stub = MagicMock()
        client._health_stub = MagicMock()
        return client

    def test_build_create_run_request_basic(self):
        client = self._make_client()
        request = client._build_create_run_request(
            flow_id="test-flow",
            input_data={"message": "hello"},
            variables=None,
            overrides=None,
            max_concurrency=None,
            wait=True,
        )
        assert request.flow_id == "test-flow"
        assert request.wait is True
        assert len(request.input) == 1

    def test_build_create_run_request_batch(self):
        client = self._make_client()
        request = client._build_create_run_request(
            flow_id="test-flow",
            input_data=[{"a": 1}, {"a": 2}],
            variables=None,
            overrides=None,
            max_concurrency=None,
            wait=False,
        )
        assert len(request.input) == 2
        assert request.wait is False

    def test_build_create_run_request_with_variables(self):
        client = self._make_client()
        request = client._build_create_run_request(
            flow_id="test-flow",
            input_data={"x": 1},
            variables={"api_key": "sk-test"},
            overrides=None,
            max_concurrency=None,
            wait=True,
        )
        assert "api_key" in request.variables
        assert request.variables["api_key"].string_value == "sk-test"

    def test_build_create_run_request_with_overrides(self):
        client = self._make_client()
        request = client._build_create_run_request(
            flow_id="test-flow",
            input_data={"x": 1},
            variables=None,
            overrides={"step1": {"value": {"input": {"temp": 0.9}}}},
            max_concurrency=None,
            wait=True,
        )
        assert request.HasField("overrides")

    def test_build_create_run_request_with_concurrency(self):
        client = self._make_client()
        request = client._build_create_run_request(
            flow_id="test-flow",
            input_data=[{"a": 1}],
            variables=None,
            overrides=None,
            max_concurrency=5,
            wait=False,
        )
        assert request.max_concurrency == 5

    def test_build_create_run_request_with_timeout(self):
        client = self._make_client()
        request = client._build_create_run_request(
            flow_id="test-flow",
            input_data={"x": 1},
            variables=None,
            overrides=None,
            max_concurrency=None,
            wait=True,
            wait_timeout=60,
        )
        assert request.timeout_secs == 60


@pytest.mark.asyncio
async def test_store_flow():
    """Test store_flow builds correct request and calls stub."""
    client = StepflowClient.__new__(StepflowClient)
    client._channel = MagicMock()
    client._runs_stub = MagicMock()
    client._health_stub = MagicMock()

    from stepflow_py.proto import flows_pb2

    mock_response = flows_pb2.StoreFlowResponse(
        flow_id="abc123",
        stored=True,
    )
    client._flows_stub = MagicMock()
    client._flows_stub.StoreFlow = AsyncMock(return_value=mock_response)

    result = await client.store_flow({"name": "test_flow", "steps": []})

    assert result.flow_id == "abc123"
    assert result.stored is True
    client._flows_stub.StoreFlow.assert_called_once()


@pytest.mark.asyncio
async def test_is_healthy_true():
    """Test is_healthy returns True when server responds."""
    from stepflow_py.proto import health_pb2

    client = StepflowClient.__new__(StepflowClient)
    client._channel = MagicMock()
    client._flows_stub = MagicMock()
    client._runs_stub = MagicMock()

    mock_response = health_pb2.HealthCheckResponse(status="healthy")
    client._health_stub = MagicMock()
    client._health_stub.HealthCheck = AsyncMock(return_value=mock_response)

    assert await client.is_healthy() is True


@pytest.mark.asyncio
async def test_is_healthy_false():
    """Test is_healthy returns False when server is unreachable."""
    import grpc

    client = StepflowClient.__new__(StepflowClient)
    client._channel = MagicMock()
    client._flows_stub = MagicMock()
    client._runs_stub = MagicMock()

    client._health_stub = MagicMock()
    client._health_stub.HealthCheck = AsyncMock(
        side_effect=grpc.aio.AioRpcError(
            code=grpc.StatusCode.UNAVAILABLE,
            initial_metadata=grpc.aio.Metadata(),
            trailing_metadata=grpc.aio.Metadata(),
            details="Connection refused",
        )
    )

    assert await client.is_healthy() is False
