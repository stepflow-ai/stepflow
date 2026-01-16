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

"""Tests for StepflowRuntime.

Unit tests verify error classes and constants without the binary.
Integration tests require the bundled stepflow-server binary.
"""

import pytest
from stepflow_core import RestartPolicy
from stepflow_runtime import StepflowRuntime, StepflowRuntimeError
from stepflow_runtime.logging import LogConfig
from stepflow_runtime.utils import get_binary_path


def binary_available() -> bool:
    """Check if the stepflow-server binary is available."""
    try:
        path = get_binary_path()
        return path.exists()
    except FileNotFoundError:
        return False


requires_binary = pytest.mark.skipif(
    not binary_available(),
    reason="Requires bundled stepflow-server binary",
)


@pytest.fixture
def runtime():
    """Create a runtime instance for testing.

    Yields a running runtime and ensures cleanup after the test.
    """
    runtime = StepflowRuntime.start()
    try:
        yield runtime
    finally:
        runtime.stop()


class TestStepflowRuntimeUnit:
    """Unit tests that don't require the binary."""

    def test_runtime_error(self):
        error = StepflowRuntimeError("Test error")
        assert str(error) == "Test error"

    def test_restart_policy_values(self):
        assert RestartPolicy.NEVER.value == "never"
        assert RestartPolicy.ON_FAILURE.value == "on_failure"
        assert RestartPolicy.ALWAYS.value == "always"


class TestStepflowRuntimeIntegration:
    """Integration tests that require the stepflow-server binary.

    These tests verify the runtime can start, manage, and communicate with
    the stepflow-server process. They use the bundled binary from the
    stepflow-runtime package.
    """

    @requires_binary
    def test_runtime_start_stop(self):
        """Test explicit start/stop lifecycle management."""
        runtime = StepflowRuntime.start()
        try:
            assert runtime.is_alive
            assert runtime.url.startswith("http://")
        finally:
            runtime.stop()
        assert not runtime.is_alive

    @requires_binary
    def test_runtime_context_manager(self):
        """Test sync context manager ensures cleanup on exit."""
        with StepflowRuntime.start() as runtime:
            assert runtime.is_alive
        assert not runtime.is_alive

    @requires_binary
    async def test_runtime_async_context_manager(self):
        """Test async context manager ensures cleanup on exit."""
        async with StepflowRuntime.start() as runtime:
            assert runtime.is_alive
        assert not runtime.is_alive


class TestStepflowRuntimeConfiguration:
    """Tests for runtime configuration options."""

    @requires_binary
    def test_runtime_with_log_config(self):
        """Test runtime accepts LogConfig for logging configuration."""
        log_config = LogConfig(level="debug", capture=True)
        with StepflowRuntime.start(log_config=log_config) as runtime:
            assert runtime.is_alive

    @requires_binary
    def test_runtime_with_restart_policy(self):
        """Test runtime accepts RestartPolicy for fault tolerance."""
        with StepflowRuntime.start(
            restart_policy=RestartPolicy.ON_FAILURE,
            max_restarts=3,
        ) as runtime:
            assert runtime.is_alive

    @requires_binary
    def test_runtime_auto_port_selection(self):
        """Test runtime auto-selects an available port."""
        with StepflowRuntime.start() as runtime:
            assert runtime.port > 0
            assert f":{runtime.port}" in runtime.url

    @requires_binary
    def test_runtime_explicit_port(self):
        """Test runtime can use a specific port."""
        with StepflowRuntime.start(port=18080) as runtime:
            assert runtime.port == 18080

    @requires_binary
    def test_runtime_log_capture(self):
        """Test runtime can capture and retrieve logs."""
        log_config = LogConfig(capture=True)
        with StepflowRuntime.start(log_config=log_config) as runtime:
            logs = runtime.get_recent_logs(limit=10)
            assert isinstance(logs, list)


class TestStepflowRuntimeExecution:
    """Tests for workflow execution through the runtime."""

    @requires_binary
    async def test_runtime_run_workflow(self):
        """Test executing a workflow end-to-end through the runtime.

        Uses a simple workflow with put_blob (a built-in component that
        doesn't require external configuration) to verify the full execution
        path: runtime start -> workflow submission -> result retrieval.
        """
        async with StepflowRuntime.start() as runtime:
            workflow = {
                "schema": "https://stepflow.org/schemas/v1/flow.json",
                "name": "test-workflow",
                "inputSchema": {
                    "type": "object",
                    "properties": {"message": {"type": "string"}},
                    "required": ["message"],
                },
                "steps": [
                    {
                        "id": "store",
                        "component": "/put_blob",
                        "input": {
                            "data": {
                                "msg": {
                                    "$from": {"workflow": "input"},
                                    "path": "$.message",
                                }
                            },
                            "blob_type": "data",
                        },
                    }
                ],
                "output": {
                    "blob_id": {"$from": {"step": "store"}, "path": "$.blob_id"}
                },
            }
            result = await runtime.run(workflow, {"message": "hello"})
            assert result.is_success, f"Workflow failed: {result.error}"
            assert result.output is not None
            assert "blob_id" in result.output
