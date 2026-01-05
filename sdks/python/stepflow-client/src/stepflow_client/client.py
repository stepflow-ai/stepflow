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

import yaml
from stepflow_api import Client
from stepflow_api.api.component import list_components as api_list_components
from stepflow_api.api.flow import (
    delete_flow as api_delete_flow,
    get_flow as api_get_flow,
    store_flow as api_store_flow,
)
from stepflow_api.api.health import health_check as api_health_check
from stepflow_api.api.run import (
    cancel_run as api_cancel_run,
    create_run as api_create_run,
    delete_run as api_delete_run,
    get_run as api_get_run,
    get_run_flow as api_get_run_flow,
    get_run_items as api_get_run_items,
    get_run_steps as api_get_run_steps,
    list_runs as api_list_runs,
)
from stepflow_api.models import (
    CreateRunRequest,
    CreateRunResponse,
    ExecutionStatus,
    Flow,
    FlowResponse,
    HealthResponse,
    ListComponentsResponse,
    ListItemsResponse,
    ListRunsResponse,
    ListStepRunsResponse,
    RunDetails,
    RunFlowResponse,
    StoreFlowRequest,
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
            from stepflow_api.api.run import get_run
            response = get_run.sync_detailed(client=client.api, run_id=run_id)
        ```
    """

    def __init__(self, base_url: str, timeout: float = 30.0):
        """Initialize the client.

        Args:
            base_url: Base URL of the Stepflow server (e.g., "http://localhost:7837")
            timeout: Request timeout in seconds
        """
        # Remove trailing slash for consistency
        self._url = base_url.rstrip("/")
        # The generated client uses paths like /health, /flows, /runs
        # but the server uses /api/v1 prefix
        api_base_url = f"{self._url}/api/v1"
        self._client = Client(base_url=api_base_url, timeout=timeout)

    @property
    def url(self) -> str:
        """Get the base URL of the Stepflow server."""
        return self._url

    @property
    def api(self) -> Client:
        """Access the low-level generated API client."""
        return self._client

    async def __aenter__(self) -> "StepflowClient":
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Async context manager exit."""
        pass

    def __enter__(self) -> "StepflowClient":
        """Sync context manager entry."""
        return self

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Sync context manager exit."""
        pass

    # =========================================================================
    # Health
    # =========================================================================

    async def health_check(self) -> HealthResponse:
        """Check server health.

        Returns:
            HealthResponse with server status
        """
        response = await api_health_check.asyncio_detailed(client=self._client)
        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Health check failed: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse health response")
        return response.parsed

    # =========================================================================
    # Flow Management
    # =========================================================================

    async def store_flow(self, flow: str | Path | dict) -> StoreFlowResponse:
        """Store a flow definition.

        Args:
            flow: Flow definition - can be:
                - Path to a YAML/JSON file (str or Path)
                - Dict containing the flow definition

        Returns:
            StoreFlowResponse with flow_id
        """
        # Load flow from file if needed
        if isinstance(flow, (str, Path)):
            path = Path(flow)
            content = path.read_text()
            if path.suffix in (".yaml", ".yml"):
                flow_dict = yaml.safe_load(content)
            else:
                flow_dict = json.loads(content)
        else:
            flow_dict = flow

        # Convert dict to Flow model
        flow_model = Flow.from_dict(flow_dict)
        request = StoreFlowRequest(flow=flow_model)
        response = await api_store_flow.asyncio_detailed(client=self._client, body=request)

        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Failed to store flow: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse store flow response")
        return response.parsed

    async def get_flow(self, flow_id: str) -> FlowResponse:
        """Get a stored flow by ID.

        Args:
            flow_id: The flow hash

        Returns:
            FlowResponse with the flow definition
        """
        response = await api_get_flow.asyncio_detailed(client=self._client, flow_id=flow_id)

        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Failed to get flow: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse get flow response")
        return response.parsed

    async def delete_flow(self, flow_id: str) -> None:
        """Delete a stored flow.

        Args:
            flow_id: The flow hash
        """
        response = await api_delete_flow.asyncio_detailed(client=self._client, flow_id=flow_id)

        if response.status_code.value not in (200, 204):
            raise StepflowClientError(
                f"Failed to delete flow: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

    # =========================================================================
    # Run Management
    # =========================================================================

    async def create_run(
        self,
        flow_id: str,
        input: Any,
        *,
        overrides: WorkflowOverrides | None = None,
        variables: dict[str, Any] | None = None,
        debug: bool = False,
        max_concurrency: int | None = None,
    ) -> CreateRunResponse:
        """Create and execute a flow run.

        Args:
            flow_id: The flow hash to execute
            input: Input data for the flow. Can be a single value or a list for batch execution.
            overrides: Optional workflow overrides
            variables: Optional variables for the workflow
            debug: Whether to run in debug mode
            max_concurrency: Max concurrency for batch runs

        Returns:
            CreateRunResponse with run_id and status
        """
        # Wrap input in a list if not already (API always expects array)
        if not isinstance(input, list):
            input = [input]

        request = CreateRunRequest(
            flow_id=flow_id,
            input_=input,
            overrides=overrides if overrides else UNSET,
            variables=variables if variables else UNSET,
            debug=debug,
            max_concurrency=max_concurrency if max_concurrency else UNSET,
        )

        response = await api_create_run.asyncio_detailed(client=self._client, body=request)

        if response.status_code.value != 200:
            # Try to extract error details from response
            try:
                details = response.content.decode() if response.content else None
            except Exception:
                details = None
            raise StepflowClientError(
                f"Failed to create run: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
                details={"response": details} if details else None,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse create run response")
        return response.parsed

    async def get_run(self, run_id: str) -> RunDetails:
        """Get run details.

        Args:
            run_id: The run ID

        Returns:
            RunDetails with run status and metadata
        """
        response = await api_get_run.asyncio_detailed(client=self._client, run_id=run_id)

        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Failed to get run: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse get run response")
        return response.parsed

    async def cancel_run(self, run_id: str) -> RunDetails:
        """Cancel a running execution.

        Args:
            run_id: The run ID

        Returns:
            RunDetails with updated status
        """
        response = await api_cancel_run.asyncio_detailed(client=self._client, run_id=run_id)

        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Failed to cancel run: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse cancel run response")
        return response.parsed

    async def delete_run(self, run_id: str) -> None:
        """Delete a run.

        Args:
            run_id: The run ID
        """
        response = await api_delete_run.asyncio_detailed(client=self._client, run_id=run_id)

        if response.status_code.value not in (200, 204):
            raise StepflowClientError(
                f"Failed to delete run: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )

    async def list_runs(
        self,
        *,
        status: ExecutionStatus | None = None,
        flow_id: str | None = None,
        limit: int | None = None,
        offset: int | None = None,
    ) -> ListRunsResponse:
        """List runs with optional filtering.

        Args:
            status: Filter by execution status
            flow_id: Filter by flow ID
            limit: Maximum number of results
            offset: Offset for pagination

        Returns:
            ListRunsResponse with list of runs
        """
        response = await api_list_runs.asyncio_detailed(
            client=self._client,
            status=status if status else UNSET,
            flow_id=flow_id if flow_id else UNSET,
            limit=limit if limit else UNSET,
            offset=offset if offset else UNSET,
        )

        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Failed to list runs: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse list runs response")
        return response.parsed

    async def get_run_items(self, run_id: str) -> ListItemsResponse:
        """Get items for a run.

        Args:
            run_id: The run ID

        Returns:
            ListItemsResponse with item results
        """
        response = await api_get_run_items.asyncio_detailed(client=self._client, run_id=run_id)

        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Failed to get run items: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse run items response")
        return response.parsed

    async def get_run_steps(self, run_id: str) -> ListStepRunsResponse:
        """Get step runs for a run.

        Args:
            run_id: The run ID

        Returns:
            ListStepRunsResponse with step execution details
        """
        response = await api_get_run_steps.asyncio_detailed(client=self._client, run_id=run_id)

        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Failed to get run steps: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse run steps response")
        return response.parsed

    async def get_run_flow(self, run_id: str) -> RunFlowResponse:
        """Get the flow used for a run.

        Args:
            run_id: The run ID

        Returns:
            RunFlowResponse with the flow definition
        """
        response = await api_get_run_flow.asyncio_detailed(client=self._client, run_id=run_id)

        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Failed to get run flow: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse run flow response")
        return response.parsed

    # =========================================================================
    # Components
    # =========================================================================

    async def list_components(self, *, include_schemas: bool = False) -> ListComponentsResponse:
        """List available components.

        Args:
            include_schemas: Whether to include input/output schemas in response

        Returns:
            ListComponentsResponse with component list
        """
        response = await api_list_components.asyncio_detailed(
            client=self._client,
            include_schemas=include_schemas if include_schemas else UNSET,
        )

        if response.status_code.value != 200:
            raise StepflowClientError(
                f"Failed to list components: HTTP {response.status_code.value}",
                status_code=response.status_code.value,
            )
        if response.parsed is None:
            raise StepflowClientError("Failed to parse list components response")
        return response.parsed
