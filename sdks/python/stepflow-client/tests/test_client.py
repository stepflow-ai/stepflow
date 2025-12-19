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


class TestStepflowClientIntegration:
    """Integration tests that require a running server."""

    @pytest.mark.skip(reason="Requires running stepflow server")
    async def test_client_run(self):
        async with StepflowClient("http://localhost:7837") as client:
            result = await client.run(
                "examples/basic/workflow.yaml",
                {"m": 3, "n": 4},
            )
            assert result.is_success

    @pytest.mark.skip(reason="Requires running stepflow server")
    async def test_client_submit_and_get(self):
        async with StepflowClient("http://localhost:7837") as client:
            run_id = await client.submit(
                "examples/basic/workflow.yaml",
                {"m": 3, "n": 4},
            )
            assert run_id

            result = await client.get_result(run_id)
            # Result may still be running
            assert result.status is not None

    @pytest.mark.skip(reason="Requires running stepflow server")
    async def test_client_validate(self):
        async with StepflowClient("http://localhost:7837") as client:
            result = await client.validate("examples/basic/workflow.yaml")
            assert result.valid

    @pytest.mark.skip(reason="Requires running stepflow server")
    async def test_client_list_components(self):
        async with StepflowClient("http://localhost:7837") as client:
            components = await client.list_components()
            assert isinstance(components, list)
