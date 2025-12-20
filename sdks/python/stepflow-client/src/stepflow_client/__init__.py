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

"""HTTP client for Stepflow servers.

This package provides an async HTTP client wrapper around the generated stepflow_api
client with quality-of-life improvements while preserving exact API semantics.

Example:
    ```python
    from stepflow_client import StepflowClient

    async with StepflowClient("http://localhost:7837") as client:
        # Store a flow (from file or dict)
        store_result = await client.store_flow("workflow.yaml")
        flow_id = store_result.flow_id

        # Create and execute a run
        run_result = await client.create_run(flow_id, {"x": 1})
        print(f"Status: {run_result.status}")

        # Access low-level API if needed
        from stepflow_api.api.flow import get_flow
        detailed = await get_flow.asyncio_detailed(client=client.api, flow_id=flow_id)
    ```

For the low-level generated API client, use `stepflow_api` directly:
    ```python
    from stepflow_api import Client
    from stepflow_api.api.flow import store_flow
    from stepflow_api.models import StoreFlowResponse
    ```
"""

from .client import StepflowClient, StepflowClientError

# Re-export commonly used types from stepflow_api for convenience
from stepflow_api.models import (
    BatchDetails,
    BatchStatus,
    CancelBatchResponse,
    CreateBatchResponse,
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

__version__ = "0.1.0"

__all__ = [
    # Client
    "StepflowClient",
    "StepflowClientError",
    # Response types
    "BatchDetails",
    "BatchStatus",
    "CancelBatchResponse",
    "CreateBatchResponse",
    "CreateRunResponse",
    "ExecutionStatus",
    "HealthResponse",
    "ListBatchesResponse",
    "ListBatchOutputsResponse",
    "ListBatchRunsResponse",
    "ListComponentsResponse",
    "ListRunsResponse",
    "ListStepRunsResponse",
    "RunDetails",
    "StoreFlowResponse",
    "WorkflowOverrides",
]
