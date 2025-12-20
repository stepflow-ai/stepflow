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

import pytest

from stepflow_client import StepflowClient, StepflowClientError


class TestStepflowClient:
    def test_client_url(self):
        client = StepflowClient("http://localhost:7837")
        assert client.url == "http://localhost:7837"

    def test_client_url_trailing_slash_removed(self):
        client = StepflowClient("http://localhost:7837/")
        assert client.url == "http://localhost:7837"

    def test_client_context_manager(self):
        # Should be able to use as context manager
        async def test():
            async with StepflowClient("http://localhost:7837") as client:
                assert client.url == "http://localhost:7837"

    def test_client_error(self):
        error = StepflowClientError("Test error")
        assert str(error) == "Test error"

    def test_client_error_with_status_code(self):
        error = StepflowClientError("Test error", status_code=404)
        assert error.status_code == 404

    def test_client_error_with_details(self):
        error = StepflowClientError("Test error", details={"key": "value"})
        assert error.details == {"key": "value"}


class TestStepflowClientIntegration:
    """Integration tests that require a running server.

    These tests are skipped by default. The stepflow-runtime package
    has comprehensive e2e tests that exercise the client through the runtime.
    """

    @pytest.mark.skip(reason="Requires running stepflow server")
    async def test_client_store_flow(self):
        """Test storing a flow definition."""
        async with StepflowClient("http://localhost:7837") as client:
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
                "output": {"result": {"$from": {"step": "echo"}}},
            }
            response = await client.store_flow(flow)
            assert response.flow_id is not None

    @pytest.mark.skip(reason="Requires running stepflow server")
    async def test_client_create_run(self):
        """Test creating and executing a run."""
        async with StepflowClient("http://localhost:7837") as client:
            # First store a flow
            flow = {
                "schema": "https://stepflow.org/schemas/v1/flow.json",
                "name": "test-flow",
                "inputSchema": {"type": "object"},
                "steps": [
                    {
                        "id": "echo",
                        "component": "/builtin/eval",
                        "input": {"expr": "'hello'"},
                    }
                ],
                "output": {"result": {"$from": {"step": "echo"}}},
            }
            store_response = await client.store_flow(flow)

            # Then create a run
            run_response = await client.create_run(
                flow_id=store_response.flow_id,
                input={},
            )
            assert run_response.run_id is not None

    @pytest.mark.skip(reason="Requires running stepflow server")
    async def test_client_get_run(self):
        """Test getting run details."""
        async with StepflowClient("http://localhost:7837") as client:
            # First store and run a flow
            flow = {
                "schema": "https://stepflow.org/schemas/v1/flow.json",
                "name": "test-flow",
                "inputSchema": {"type": "object"},
                "steps": [
                    {
                        "id": "echo",
                        "component": "/builtin/eval",
                        "input": {"expr": "'hello'"},
                    }
                ],
                "output": {"result": {"$from": {"step": "echo"}}},
            }
            store_response = await client.store_flow(flow)
            run_response = await client.create_run(
                flow_id=store_response.flow_id,
                input={},
            )

            # Then get the run details
            details = await client.get_run(str(run_response.run_id))
            assert details.status is not None

    @pytest.mark.skip(reason="Requires running stepflow server")
    async def test_client_list_components(self):
        """Test listing available components."""
        async with StepflowClient("http://localhost:7837") as client:
            response = await client.list_components()
            assert response.components is not None

    @pytest.mark.skip(reason="Requires running stepflow server")
    async def test_client_health(self):
        """Test health check endpoint."""
        async with StepflowClient("http://localhost:7837") as client:
            response = await client.health()
            assert response.status == "ok"
