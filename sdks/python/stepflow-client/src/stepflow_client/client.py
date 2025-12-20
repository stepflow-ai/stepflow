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

"""HTTP client wrapper for Stepflow servers.

This module provides a higher-level wrapper around the generated stepflow_api
client with quality-of-life improvements like file loading and simpler method signatures.

The wrapper preserves API semantics exactly - all methods correspond directly to
API endpoints and return API response types.
"""

import json
from pathlib import Path
from typing import Any

import httpx
import yaml

from stepflow_api import Client
from stepflow_api.api.batch import (
    cancel_batch,
    create_batch,
    get_batch,
    get_batch_outputs,
    list_batch_runs,
    list_batches,
)
from stepflow_api.api.component import list_components
from stepflow_api.api.flow import delete_flow, get_flow, store_flow
from stepflow_api.api.health import health_check
from stepflow_api.api.run import cancel_run, create_run, get_run, get_run_steps, list_runs
from stepflow_api.models import (
    BatchDetails,
    BatchStatus,
    CancelBatchResponse,
    CreateBatchRequest,
    CreateBatchResponse,
    CreateRunRequest,
    CreateRunResponse,
    ExecutionStatus,
    HealthResponse,
    ListBatchesResponse,
    ListBatchOutputsResponse,
    ListBatchRunsResponse,
    ListComponentsResponse,
    ListRunsResponse,
    ListStepRunsResponse,
    RunDetails,
    StoreFlowResponse,
    WorkflowOverrides,
)
from stepflow_api.types import UNSET


class StepflowClientError(Exception):
    """Error from the Stepflow client."""

    def __init__(self, message: str, status_code: int | None = None, details: dict | None = None):
        super().__init__(message)
        self.status_code = status_code
        self.details = details


class StepflowClient:
    """HTTP client wrapper for Stepflow servers.

    Provides a higher-level interface around the generated API client with
    quality-of-life improvements while preserving exact API semantics.

    QoL improvements:
    - Load flows from file paths (YAML/JSON)
    - Simpler method signatures with sensible defaults
    - Context manager support for resource cleanup

    All methods return API response types directly. For the low-level generated
    client, access the `.api` property.

    Example:
        ```python
        async with StepflowClient("http://localhost:7837") as client:
            # Store a flow (from file or dict)
            store_response = await client.store_flow("workflow.yaml")
            flow_id = store_response.flow_id

            # Create and execute a run
            run_response = await client.create_run(flow_id, {"x": 1})
            print(f"Run status: {run_response.status}")

            # Access low-level API if needed
            from stepflow_api.api.flow import get_flow
            response = await get_flow.asyncio_detailed(client=client.api, flow_id=flow_id)
        ```
    """

    def __init__(
        self,
        url: str = "http://localhost:7837",
        *,
        timeout: float = 300.0,
        headers: dict[str, str] | None = None,
    ):
        """Initialize the Stepflow client.

        Args:
            url: Base URL of the Stepflow server (e.g., "http://localhost:7837")
            timeout: Request timeout in seconds (default: 300s for long workflows)
            headers: Optional additional headers to include in requests
        """
        # Ensure URL ends with /api/v1
        base_url = url.rstrip("/")
        if not base_url.endswith("/api/v1"):
            base_url = f"{base_url}/api/v1"

        self._url = url.rstrip("/")
        self._client = Client(
            base_url=base_url,
            timeout=httpx.Timeout(timeout),
            headers=headers or {},
        )

    @property
    def url(self) -> str:
        """Base URL of the Stepflow server."""
        return self._url

    @property
    def api(self) -> Client:
        """Access the low-level generated API client.

        Use this property when you need direct access to the generated API
        methods or want to use features not exposed by the wrapper.

        Example:
            ```python
            # Use wrapper methods for common operations
            response = await client.store_flow("workflow.yaml")

            # Use low-level API for advanced operations
            from stepflow_api.api.flow import store_flow
            detailed = await store_flow.asyncio_detailed(
                client=client.api,
                flow=my_flow_dict
            )
            print(f"Headers: {detailed.headers}")
            ```
        """
        return self._client

    async def close(self) -> None:
        """Close the HTTP client and release resources."""
        await self._client.get_async_httpx_client().aclose()

    async def __aenter__(self) -> "StepflowClient":
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Async context manager exit."""
        await self.close()

    def _load_flow(self, flow: str | Path | dict[str, Any]) -> dict[str, Any]:
        """Load a flow from file path or return as-is if already a dict.

        Args:
            flow: Path to a YAML/JSON workflow file, or a dict

        Returns:
            The flow as a dictionary

        Raises:
            FileNotFoundError: If the file doesn't exist
        """
        if isinstance(flow, dict):
            return flow
        path = Path(flow)
        if not path.exists():
            raise FileNotFoundError(f"Workflow file not found: {path}")
        with open(path) as f:
            return yaml.safe_load(f)

    # =========================================================================
    # Flow endpoints
    # =========================================================================

    async def store_flow(
        self,
        flow: str | Path | dict[str, Any],
    ) -> StoreFlowResponse:
        """Store a flow definition.

        Args:
            flow: Path to a YAML/JSON workflow file, or flow dict

        Returns:
            StoreFlowResponse with flow_id and diagnostics

        Raises:
            StepflowClientError: If the request fails
        """
        flow_dict = self._load_flow(flow)
        response = await store_flow.asyncio_detailed(client=self._client, flow=flow_dict)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to store flow: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse store flow response")

        return response.parsed

    async def get_flow(self, flow_id: str) -> dict[str, Any]:
        """Get a stored flow by ID.

        Args:
            flow_id: The flow ID

        Returns:
            Flow response as a dictionary (raw API response)
        """
        response = await get_flow.asyncio_detailed(client=self._client, flow_id=flow_id)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to get flow: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        # Response is raw JSON since generator couldn't create proper model
        return json.loads(response.content)

    async def delete_flow(self, flow_id: str) -> None:
        """Delete a stored flow.

        Args:
            flow_id: The flow ID to delete
        """
        response = await delete_flow.asyncio_detailed(client=self._client, flow_id=flow_id)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to delete flow: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

    # =========================================================================
    # Run endpoints
    # =========================================================================

    async def create_run(
        self,
        flow_id: str,
        input: dict[str, Any],
        *,
        debug: bool = False,
        overrides: WorkflowOverrides | None = None,
        variables: dict[str, Any] | None = None,
    ) -> CreateRunResponse:
        """Create and execute a run.

        Args:
            flow_id: The flow ID to execute
            input: Input data for the workflow
            debug: Whether to run in debug mode
            overrides: Optional workflow overrides
            variables: Optional workflow variables

        Returns:
            CreateRunResponse with run_id, status, and result
        """
        request = CreateRunRequest(
            flow_id=flow_id,
            input_=input,
            debug=debug,  # Always pass the boolean value
            overrides=overrides if overrides is not None else UNSET,
            variables=variables if variables is not None else UNSET,
        )
        response = await create_run.asyncio_detailed(client=self._client, body=request)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to create run: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse create run response")

        return response.parsed

    async def get_run(self, run_id: str) -> RunDetails:
        """Get details of a run.

        Args:
            run_id: The run ID

        Returns:
            RunDetails with status and result
        """
        response = await get_run.asyncio_detailed(client=self._client, run_id=run_id)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to get run: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse get run response")

        return response.parsed

    async def cancel_run(self, run_id: str) -> RunDetails:
        """Cancel a running workflow.

        Args:
            run_id: The run ID to cancel

        Returns:
            RunDetails with updated status
        """
        response = await cancel_run.asyncio_detailed(client=self._client, run_id=run_id)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to cancel run: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse cancel run response")

        return response.parsed

    async def list_runs(
        self,
        *,
        flow_id: str | None = None,
        limit: int | None = None,
        offset: int | None = None,
    ) -> ListRunsResponse:
        """List runs with optional filtering.

        Args:
            flow_id: Filter by flow ID
            limit: Maximum number of results
            offset: Number of results to skip

        Returns:
            ListRunsResponse with list of run summaries
        """
        response = await list_runs.asyncio_detailed(
            client=self._client,
            flow_id=flow_id,
            limit=limit,
            offset=offset,
        )

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to list runs: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse list runs response")

        return response.parsed

    async def get_run_steps(self, run_id: str) -> ListStepRunsResponse:
        """Get step-level details of a run.

        Args:
            run_id: The run ID

        Returns:
            ListStepRunsResponse with step details
        """
        response = await get_run_steps.asyncio_detailed(client=self._client, run_id=run_id)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to get run steps: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse get run steps response")

        return response.parsed

    # =========================================================================
    # Batch endpoints
    # =========================================================================

    async def create_batch(
        self,
        flow_id: str,
        inputs: list[dict[str, Any]],
        *,
        max_concurrency: int | None = None,
        overrides: WorkflowOverrides | None = None,
    ) -> CreateBatchResponse:
        """Create a batch of runs.

        Args:
            flow_id: The flow ID to execute
            inputs: List of input data for each run
            max_concurrency: Maximum concurrent executions
            overrides: Optional workflow overrides

        Returns:
            CreateBatchResponse with batch_id
        """
        request = CreateBatchRequest(
            flow_id=flow_id,
            inputs=inputs,
            max_concurrency=max_concurrency if max_concurrency is not None else UNSET,
            overrides=overrides if overrides is not None else UNSET,
        )
        response = await create_batch.asyncio_detailed(client=self._client, body=request)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to create batch: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse create batch response")

        return response.parsed

    async def get_batch(self, batch_id: str) -> BatchDetails:
        """Get batch details.

        Args:
            batch_id: The batch ID

        Returns:
            BatchDetails with batch status and statistics
        """
        response = await get_batch.asyncio_detailed(client=self._client, batch_id=batch_id)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to get batch: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse get batch response")

        return response.parsed

    async def get_batch_outputs(self, batch_id: str) -> ListBatchOutputsResponse:
        """Get outputs from a batch.

        Args:
            batch_id: The batch ID

        Returns:
            ListBatchOutputsResponse with individual run results
        """
        response = await get_batch_outputs.asyncio_detailed(
            client=self._client, batch_id=batch_id
        )

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to get batch outputs: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse get batch outputs response")

        return response.parsed

    async def list_batch_runs(self, batch_id: str) -> ListBatchRunsResponse:
        """List runs in a batch.

        Args:
            batch_id: The batch ID

        Returns:
            ListBatchRunsResponse with list of batch run info
        """
        response = await list_batch_runs.asyncio_detailed(client=self._client, batch_id=batch_id)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to list batch runs: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse list batch runs response")

        return response.parsed

    async def cancel_batch(self, batch_id: str) -> CancelBatchResponse:
        """Cancel a batch.

        Args:
            batch_id: The batch ID to cancel

        Returns:
            CancelBatchResponse with updated status
        """
        response = await cancel_batch.asyncio_detailed(client=self._client, batch_id=batch_id)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to cancel batch: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse cancel batch response")

        return response.parsed

    async def list_batches(
        self,
        *,
        flow_name: str | None = None,
        status: BatchStatus | None = None,
        limit: int | None = None,
        offset: int | None = None,
    ) -> ListBatchesResponse:
        """List batches with optional filtering.

        Args:
            flow_name: Filter by flow name
            status: Filter by batch status
            limit: Maximum number of results
            offset: Number of results to skip

        Returns:
            ListBatchesResponse with list of batch metadata
        """
        response = await list_batches.asyncio_detailed(
            client=self._client,
            flow_name=flow_name,
            status=status,
            limit=limit,
            offset=offset,
        )

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to list batches: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse list batches response")

        return response.parsed

    # =========================================================================
    # Component endpoints
    # =========================================================================

    async def list_components(self, *, include_schemas: bool = True) -> ListComponentsResponse:
        """List available components.

        Args:
            include_schemas: Whether to include input/output schemas

        Returns:
            ListComponentsResponse with list of components
        """
        response = await list_components.asyncio_detailed(
            client=self._client,
            include_schemas=include_schemas,
        )

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Failed to list components: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse list components response")

        return response.parsed

    # =========================================================================
    # Health endpoints
    # =========================================================================

    async def health(self) -> HealthResponse:
        """Check server health.

        Returns:
            HealthResponse with status, version, and timestamp
        """
        response = await health_check.asyncio_detailed(client=self._client)

        if response.status_code.value >= 400:
            raise StepflowClientError(
                f"Health check failed: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

        if response.parsed is None:
            raise StepflowClientError("Failed to parse health response")

        return response.parsed
