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

"""End-to-end tests for StepflowRuntime.

These tests verify the interaction between the Python client/runtime and the
stepflow-worker. They are intentionally minimal - testing client behavior,
not server-side workflow logic (which has its own test suite).

Test categories:
- Lifecycle: Runtime start/stop, context managers
- Workflow execution: run, submit, get_result
- Validation: validate workflows
- Batch operations: submit_batch, get_batch
- Smoke tests: Basic workflow execution as sanity check
"""

import pytest
from stepflow_runtime import StepflowRuntime
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


# Simple workflow for testing - stores data and returns blob_id
STORE_DATA_WORKFLOW = {
    "schema": "https://stepflow.org/schemas/v1/flow.json",
    "name": "store-data",
    "inputSchema": {
        "type": "object",
        "properties": {
            "message": {"type": "string"},
            "value": {"type": "number"},
        },
        "required": ["message", "value"],
    },
    "steps": [
        {
            "id": "store",
            "component": "/put_blob",
            "input": {
                "data": {
                    "stored_message": {"$input": "message"},
                    "stored_value": {"$input": "value"},
                },
                "blob_type": "data",
            },
        },
    ],
    "output": {
        "blob_id": {"$step": "store", "path": "$.blob_id"},
    },
}


@requires_binary
class TestRuntimeLifecycle:
    """Tests for runtime startup, shutdown, and lifecycle management."""

    async def test_start_and_stop(self):
        """Test that runtime can be started and stopped."""
        runtime = await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        )
        try:
            assert runtime.is_alive
            assert runtime.port > 0
        finally:
            await runtime.stop_async()

        assert not runtime.is_alive

    async def test_async_context_manager(self):
        """Test that async context manager properly manages lifecycle."""
        async with await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        ) as runtime:
            assert runtime.is_alive
            # Verify we can make requests
            health = await runtime.health()
            assert "status" in health

        # After exiting context, runtime should be stopped
        assert not runtime.is_alive

    async def test_health_check(self):
        """Test the health check endpoint."""
        async with await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        ) as runtime:
            health = await runtime.health()

            # Check structure, not exact values (less brittle)
            assert health is not None
            assert "status" in health


@requires_binary
class TestWorkflowExecution:
    """Tests for workflow execution interactions."""

    @pytest.fixture
    async def runtime(self):
        """Create and start a runtime for testing."""
        runtime = await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        )
        yield runtime
        await runtime.stop_async()

    async def test_run_workflow(self, runtime):
        """Test running a workflow and getting result."""
        result = await runtime.run(
            STORE_DATA_WORKFLOW,
            {"message": "test", "value": 42},
        )

        assert result.is_success, f"Workflow failed: {result.error}"
        assert result.output is not None
        assert "blob_id" in result.output
        assert isinstance(result.output["blob_id"], str)

    async def test_submit_and_get_result(self, runtime):
        """Test async submit and result retrieval."""
        run_id = await runtime.submit(
            STORE_DATA_WORKFLOW,
            {"message": "async test", "value": 100},
        )

        assert run_id is not None
        assert isinstance(run_id, str)

        result = await runtime.get_result(run_id)

        assert result.is_success, f"Workflow failed: {result.error}"
        assert "blob_id" in result.output

    async def test_run_with_overrides(self, runtime):
        """Test running a workflow with step overrides."""
        # Run without overrides
        result1 = await runtime.run(
            STORE_DATA_WORKFLOW,
            {"message": "original", "value": 1},
        )
        assert result1.is_success

        # Run with overrides that change the stored data
        overrides = {
            "store": {
                "value": {
                    "input": {
                        "data": {"overridden": True},
                    },
                },
            },
        }
        result2 = await runtime.run(
            STORE_DATA_WORKFLOW,
            {"message": "original", "value": 1},
            overrides=overrides,
        )
        assert result2.is_success

        # Different data should produce different blob IDs
        assert result1.output["blob_id"] != result2.output["blob_id"]

    async def test_workflow_failure_returns_error(self, runtime):
        """Test that workflow failures return proper error information."""
        workflow = {
            "schema": "https://stepflow.org/schemas/v1/flow.json",
            "name": "fail-workflow",
            "inputSchema": {
                "type": "object",
                "properties": {"blob_id": {"type": "string"}},
                "required": ["blob_id"],
            },
            "steps": [
                {
                    "id": "retrieve",
                    "component": "/get_blob",
                    "input": {
                        "blob_id": {"$input": "blob_id"},
                    },
                },
            ],
            "output": {"$step": "retrieve", "path": "$.data"},
        }

        result = await runtime.run(workflow, {"blob_id": "nonexistent-id"})

        assert not result.is_success
        assert result.error is not None


@requires_binary
class TestValidation:
    """Tests for workflow validation."""

    @pytest.fixture
    async def runtime(self):
        """Create and start a runtime for testing."""
        runtime = await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        )
        yield runtime
        await runtime.stop_async()

    async def test_validate_valid_workflow(self, runtime):
        """Test validating a valid workflow."""
        validation = await runtime.validate(STORE_DATA_WORKFLOW)
        assert validation.valid

    async def test_validate_workflow_with_issues(self, runtime):
        """Test that validation returns diagnostics for issues."""
        # Workflow missing description - should produce a diagnostic
        workflow_with_issues = {
            "schema": "https://stepflow.org/schemas/v1/flow.json",
            "name": "test",
            "steps": [
                {
                    "id": "store",
                    "component": "/put_blob",
                    "input": {"data": {}, "blob_type": "data"},
                },
            ],
            "output": {"$step": "store"},
        }

        validation = await runtime.validate(workflow_with_issues)

        # Should have diagnostics (warnings about missing description, etc.)
        assert len(validation.diagnostics) > 0


@requires_binary
class TestBatchOperations:
    """Tests for batch workflow operations."""

    @pytest.fixture
    async def runtime(self):
        """Create and start a runtime for testing."""
        runtime = await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        )
        yield runtime
        await runtime.stop_async()

    async def test_batch_submit_and_get_with_results(self, runtime):
        """Test batch submission with result retrieval."""
        inputs = [
            {"message": "item 1", "value": 10},
            {"message": "item 2", "value": 20},
            {"message": "item 3", "value": 30},
        ]

        batch_id = await runtime.submit_batch(STORE_DATA_WORKFLOW, inputs)

        assert batch_id is not None
        assert isinstance(batch_id, str)

        details, results = await runtime.get_batch(batch_id, include_results=True)

        assert details is not None
        assert results is not None
        assert len(results) == 3

        for result in results:
            assert result.is_success
            assert "blob_id" in result.output

    async def test_batch_get_without_results(self, runtime):
        """Test getting batch details without results."""
        inputs = [{"message": "item", "value": 1}]

        batch_id = await runtime.submit_batch(STORE_DATA_WORKFLOW, inputs)
        details, results = await runtime.get_batch(batch_id, include_results=False)

        assert details is not None
        assert results is None


@requires_binary
class TestComponentDiscovery:
    """Tests for component listing and discovery."""

    @pytest.fixture
    async def runtime(self):
        """Create and start a runtime for testing."""
        runtime = await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        )
        yield runtime
        await runtime.stop_async()

    async def test_list_components(self, runtime):
        """Test that we can list available components."""
        components = await runtime.list_components()

        assert len(components) > 0
        paths = [c.path for c in components]
        # Verify some expected builtin components exist
        assert "/put_blob" in paths
        assert "/get_blob" in paths


@requires_binary
class TestSmokeTests:
    """Minimal smoke tests that exercise actual workflow execution.

    These tests verify basic server-side workflow logic as a sanity check.
    They are intentionally simple and should rarely need updating.
    """

    @pytest.fixture
    async def runtime(self):
        """Create and start a runtime for testing."""
        runtime = await StepflowRuntime.start_async(
            log_config=LogConfig(level="warn", capture=False),
            startup_timeout=30.0,
        )
        yield runtime
        await runtime.stop_async()

    async def test_data_round_trip(self, runtime):
        """Smoke test: Store and retrieve data to verify round-trip works."""
        workflow = {
            "schema": "https://stepflow.org/schemas/v1/flow.json",
            "name": "round-trip",
            "inputSchema": {
                "type": "object",
                "properties": {"data": {"type": "object"}},
                "required": ["data"],
            },
            "steps": [
                {
                    "id": "store",
                    "component": "/put_blob",
                    "input": {
                        "data": {"$input": "data"},
                        "blob_type": "data",
                    },
                },
                {
                    "id": "retrieve",
                    "component": "/get_blob",
                    "input": {
                        "blob_id": {"$step": "store", "path": "$.blob_id"},
                    },
                },
            ],
            "output": {"$step": "retrieve", "path": "$.data"},
        }

        input_data = {"key": "value", "numbers": [1, 2, 3]}
        result = await runtime.run(workflow, {"data": input_data})

        assert result.is_success
        assert result.output == input_data

    async def test_multi_step_workflow(self, runtime):
        """Smoke test: Verify multi-step workflows with data dependencies work."""
        workflow = {
            "schema": "https://stepflow.org/schemas/v1/flow.json",
            "name": "multi-step",
            "inputSchema": {
                "type": "object",
                "properties": {"items": {"type": "array"}},
                "required": ["items"],
            },
            "steps": [
                {
                    "id": "store1",
                    "component": "/put_blob",
                    "input": {
                        "data": {"$input": "items"},
                        "blob_type": "data",
                    },
                },
                {
                    "id": "store2",
                    "component": "/put_blob",
                    "input": {
                        "data": {
                            "first_blob": {"$step": "store1", "path": "$.blob_id"},
                        },
                        "blob_type": "data",
                    },
                },
            ],
            "output": {
                "blob1": {"$step": "store1", "path": "$.blob_id"},
                "blob2": {"$step": "store2", "path": "$.blob_id"},
            },
        }

        result = await runtime.run(workflow, {"items": [1, 2, 3]})

        assert result.is_success
        assert "blob1" in result.output
        assert "blob2" in result.output
        # The two blobs should be different (different data)
        assert result.output["blob1"] != result.output["blob2"]
