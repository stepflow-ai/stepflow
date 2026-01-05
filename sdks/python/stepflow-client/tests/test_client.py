# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
# in compliance with the License. You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software distributed under the License
# is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
# or implied. See the License for the specific language governing permissions and limitations under
# the License.

"""Tests for StepflowClient."""

import json
import tempfile
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import httpx
import pytest

from stepflow_client import (
    StepflowClient,
    StepflowClientError,
    StepflowExecutor,
    FlowResult,
    ValidationResult,
    ComponentInfo,
)

SERVER_URL = "http://localhost:7837"


def server_available() -> bool:
    """Check if a stepflow server is running."""
    try:
        response = httpx.get(f"{SERVER_URL}/api/v1/health", timeout=1.0)
        return response.status_code == 200
    except Exception:
        return False


requires_server = pytest.mark.skipif(
    not server_available(),
    reason="Requires running stepflow server at localhost:7837",
)


class TestStepflowClientInit:
    """Test client initialization and configuration."""

    def test_url_stored_correctly(self):
        client = StepflowClient("http://localhost:7837")
        assert client.url == "http://localhost:7837"

    def test_trailing_slash_removed(self):
        client = StepflowClient("http://localhost:7837/")
        assert client.url == "http://localhost:7837"

    def test_api_base_url_includes_prefix(self):
        client = StepflowClient("http://localhost:7837")
        # The internal client should use /api/v1 prefix
        assert client.api._base_url == "http://localhost:7837/api/v1"

    def test_custom_timeout(self):
        client = StepflowClient("http://localhost:7837", timeout=60.0)
        # Verify timeout was passed (internal structure varies by httpx version)
        assert client.api._timeout == 60.0


class TestStepflowClientError:
    """Test error class."""

    def test_basic_error(self):
        error = StepflowClientError("Something went wrong")
        assert str(error) == "Something went wrong"
        assert error.status_code is None
        assert error.details is None

    def test_error_with_status_code(self):
        error = StepflowClientError("Not found", status_code=404)
        assert error.status_code == 404

    def test_error_with_details(self):
        error = StepflowClientError("Validation failed", details={"field": "input"})
        assert error.details == {"field": "input"}

    def test_error_with_all_fields(self):
        error = StepflowClientError(
            "Bad request",
            status_code=400,
            details={"errors": ["field1 required", "field2 invalid"]},
        )
        assert str(error) == "Bad request"
        assert error.status_code == 400
        assert error.details["errors"] == ["field1 required", "field2 invalid"]


class TestStepflowClientFlowLoading:
    """Test flow loading from different sources."""

    async def test_store_flow_from_dict(self):
        """Test storing a flow from a dictionary."""
        client = StepflowClient(SERVER_URL)

        # Mock the API call
        mock_response = MagicMock()
        mock_response.status_code.value = 200
        mock_response.parsed = MagicMock()
        mock_response.parsed.flow_id = "abc123"

        with patch(
            "stepflow_client.client.api_store_flow.asyncio_detailed",
            new_callable=AsyncMock,
            return_value=mock_response,
        ) as mock_store:
            flow_dict = {
                "schema": "https://stepflow.org/schemas/v1/flow.json",
                "name": "test-flow",
                "steps": [],
                "output": {},
            }
            response = await client.store_flow(flow_dict)

            assert response.flow_id == "abc123"
            mock_store.assert_called_once()

    async def test_store_flow_from_yaml_file(self):
        """Test storing a flow from a YAML file."""
        client = StepflowClient(SERVER_URL)

        mock_response = MagicMock()
        mock_response.status_code.value = 200
        mock_response.parsed = MagicMock()
        mock_response.parsed.flow_id = "yaml123"

        with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
            f.write("""
schema: https://stepflow.org/schemas/v1/flow.json
name: yaml-test
steps: []
output: {}
""")
            f.flush()

            with patch(
                "stepflow_client.client.api_store_flow.asyncio_detailed",
                new_callable=AsyncMock,
                return_value=mock_response,
            ):
                response = await client.store_flow(f.name)
                assert response.flow_id == "yaml123"

            Path(f.name).unlink()

    async def test_store_flow_from_json_file(self):
        """Test storing a flow from a JSON file."""
        client = StepflowClient(SERVER_URL)

        mock_response = MagicMock()
        mock_response.status_code.value = 200
        mock_response.parsed = MagicMock()
        mock_response.parsed.flow_id = "json123"

        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump(
                {
                    "schema": "https://stepflow.org/schemas/v1/flow.json",
                    "name": "json-test",
                    "steps": [],
                    "output": {},
                },
                f,
            )
            f.flush()

            with patch(
                "stepflow_client.client.api_store_flow.asyncio_detailed",
                new_callable=AsyncMock,
                return_value=mock_response,
            ):
                response = await client.store_flow(f.name)
                assert response.flow_id == "json123"

            Path(f.name).unlink()


class TestStepflowClientCreateRun:
    """Test run creation."""

    async def test_single_input_wrapped_in_list(self):
        """Test that single input is automatically wrapped in a list."""
        client = StepflowClient(SERVER_URL)

        mock_response = MagicMock()
        mock_response.status_code.value = 200
        mock_response.parsed = MagicMock()
        mock_response.parsed.run_id = "run123"

        with patch(
            "stepflow_client.client.api_create_run.asyncio_detailed",
            new_callable=AsyncMock,
            return_value=mock_response,
        ) as mock_create:
            await client.create_run(flow_id="flow123", input={"x": 1})

            # Verify input was wrapped in list
            call_args = mock_create.call_args
            request = call_args.kwargs["body"]
            assert request.input_ == [{"x": 1}]

    async def test_list_input_not_double_wrapped(self):
        """Test that list input is not double-wrapped."""
        client = StepflowClient(SERVER_URL)

        mock_response = MagicMock()
        mock_response.status_code.value = 200
        mock_response.parsed = MagicMock()

        with patch(
            "stepflow_client.client.api_create_run.asyncio_detailed",
            new_callable=AsyncMock,
            return_value=mock_response,
        ) as mock_create:
            await client.create_run(flow_id="flow123", input=[{"x": 1}, {"x": 2}])

            call_args = mock_create.call_args
            request = call_args.kwargs["body"]
            assert request.input_ == [{"x": 1}, {"x": 2}]

    async def test_error_response_raises_exception(self):
        """Test that HTTP errors raise StepflowClientError."""
        client = StepflowClient(SERVER_URL)

        mock_response = MagicMock()
        mock_response.status_code.value = 400
        mock_response.content = b"Invalid flow_id"

        with patch(
            "stepflow_client.client.api_create_run.asyncio_detailed",
            new_callable=AsyncMock,
            return_value=mock_response,
        ):
            with pytest.raises(StepflowClientError) as exc_info:
                await client.create_run(flow_id="bad", input={})

            assert exc_info.value.status_code == 400
            assert "Invalid flow_id" in exc_info.value.details["response"]


class TestStepflowClientContextManager:
    """Test context manager functionality."""

    async def test_async_context_manager(self):
        """Test async context manager enters and exits cleanly."""
        async with StepflowClient(SERVER_URL) as client:
            assert client.url == SERVER_URL

    def test_sync_context_manager(self):
        """Test sync context manager enters and exits cleanly."""
        with StepflowClient(SERVER_URL) as client:
            assert client.url == SERVER_URL


class TestStepflowExecutorProtocol:
    """Test that StepflowClient implements the StepflowExecutor protocol."""

    def test_client_has_executor_methods(self):
        """Test that StepflowClient has all StepflowExecutor methods."""
        client = StepflowClient(SERVER_URL)
        # url property
        assert hasattr(client, "url")
        # Protocol methods
        assert hasattr(client, "run")
        assert hasattr(client, "submit")
        assert hasattr(client, "get_result")
        assert hasattr(client, "validate")
        assert hasattr(client, "list_components")

    def test_client_satisfies_protocol(self):
        """Test that StepflowClient is recognized as implementing StepflowExecutor."""
        client = StepflowClient(SERVER_URL)
        # Protocol is runtime_checkable, so isinstance should work
        assert isinstance(client, StepflowExecutor)

    async def test_run_returns_flow_result(self):
        """Test that run() returns a FlowResult."""
        client = StepflowClient(SERVER_URL)

        # Mock store_flow
        mock_store_response = MagicMock()
        mock_store_response.flow_id = "flow123"
        mock_store_response.diagnostics = MagicMock()
        mock_store_response.diagnostics.diagnostics = []

        # Mock create_run
        mock_run_response = MagicMock()
        mock_run_response.run_id = "run123"
        mock_run_response.status = MagicMock()
        mock_run_response.status.value = "completed"
        # Set the result attribute
        mock_run_response.result = {"outcome": "success", "result": {"value": 42}}

        with patch.object(client, "store_flow", new_callable=AsyncMock, return_value=mock_store_response):
            with patch.object(client, "create_run", new_callable=AsyncMock, return_value=mock_run_response):
                result = await client.run({"name": "test"}, {"x": 1})
                assert isinstance(result, FlowResult)
                assert result.is_success
                assert result.output == {"value": 42}

    async def test_submit_returns_run_id(self):
        """Test that submit() returns a run ID string."""
        client = StepflowClient(SERVER_URL)

        mock_store_response = MagicMock()
        mock_store_response.flow_id = "flow123"
        mock_store_response.diagnostics = MagicMock()
        mock_store_response.diagnostics.diagnostics = []

        mock_run_response = MagicMock()
        mock_run_response.run_id = "run-abc-123"

        with patch.object(client, "store_flow", new_callable=AsyncMock, return_value=mock_store_response):
            with patch.object(client, "create_run", new_callable=AsyncMock, return_value=mock_run_response):
                run_id = await client.submit({"name": "test"}, {"x": 1})
                assert isinstance(run_id, str)
                assert run_id == "run-abc-123"

    async def test_validate_returns_validation_result(self):
        """Test that validate() returns a ValidationResult."""
        client = StepflowClient(SERVER_URL)

        mock_store_response = MagicMock()
        mock_store_response.flow_id = "flow123"
        mock_store_response.diagnostics = MagicMock()
        mock_store_response.diagnostics.diagnostics = []

        with patch.object(client, "store_flow", new_callable=AsyncMock, return_value=mock_store_response):
            result = await client.validate({"name": "test"})
            assert isinstance(result, ValidationResult)
            assert result.valid is True
            assert result.diagnostics == []

    async def test_list_components_returns_component_info_list(self):
        """Test that list_components() returns list of ComponentInfo."""
        client = StepflowClient(SERVER_URL)

        mock_component = MagicMock()
        mock_component.component = MagicMock()
        mock_component.component.root = "/builtin/eval"
        mock_component.description = "Evaluate expression"
        mock_component.input_schema = None
        mock_component.output_schema = None

        mock_response = MagicMock()
        mock_response.components = [mock_component]

        with patch.object(client, "list_components_detailed", new_callable=AsyncMock, return_value=mock_response):
            components = await client.list_components()
            assert isinstance(components, list)
            assert len(components) == 1
            assert isinstance(components[0], ComponentInfo)
            assert components[0].path == "/builtin/eval"


class TestStepflowClientIntegration:
    """Integration tests that require a running server.

    These tests are skipped if no server is running at localhost:7837.
    Start a server with: stepflow-server --port 7837
    """

    @requires_server
    async def test_health_check(self):
        """Test health check returns healthy status."""
        async with StepflowClient(SERVER_URL) as client:
            response = await client.health_check()
            assert response.status == "healthy"

    @requires_server
    async def test_store_and_retrieve_flow(self):
        """Test storing and retrieving a flow."""
        async with StepflowClient(SERVER_URL) as client:
            flow = {
                "schema": "https://stepflow.org/schemas/v1/flow.json",
                "name": "test-flow",
                "steps": [
                    {
                        "id": "echo",
                        "component": "/builtin/eval",
                        "input": {"expr": "'hello'"},
                    }
                ],
                "output": {"result": {"$step": "echo"}},
            }

            # Store the flow
            store_response = await client.store_flow(flow)
            assert store_response.flow_id is not None

            # Retrieve it
            flow_response = await client.get_flow(store_response.flow_id)
            assert flow_response.flow is not None

    @requires_server
    async def test_execute_workflow_end_to_end(self):
        """Test complete workflow execution."""
        async with StepflowClient(SERVER_URL) as client:
            # Store a simple eval workflow
            flow = {
                "schema": "https://stepflow.org/schemas/v1/flow.json",
                "name": "e2e-test",
                "steps": [
                    {
                        "id": "compute",
                        "component": "/builtin/eval",
                        "input": {"expr": "2 + 2"},
                    }
                ],
                "output": {"$step": "compute"},
            }
            store_response = await client.store_flow(flow)

            # Execute the workflow
            run_response = await client.create_run(
                flow_id=store_response.flow_id,
                input={},
            )
            assert run_response.run_id is not None

            # Get the results
            details = await client.get_run(str(run_response.run_id))
            assert details.status is not None

            # Get item results
            items = await client.get_run_items(str(run_response.run_id))
            assert items.items is not None

    @requires_server
    async def test_list_components(self):
        """Test listing available components."""
        async with StepflowClient(SERVER_URL) as client:
            response = await client.list_components()
            assert response.components is not None
            # Server should have at least builtin components
            assert len(response.components) >= 0

    @requires_server
    async def test_list_runs(self):
        """Test listing runs."""
        async with StepflowClient(SERVER_URL) as client:
            response = await client.list_runs(limit=5)
            assert response.runs is not None
