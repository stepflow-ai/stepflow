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

"""HTTP client for Stepflow servers.

This package provides an async HTTP client that implements the StepflowExecutor
protocol, enabling interoperability with StepflowRuntime.

Example:
    ```python
    from stepflow_client import StepflowClient

    async with StepflowClient("http://localhost:7837") as client:
        # High-level API (StepflowExecutor protocol)
        result = await client.run("workflow.yaml", {"x": 1})
        if result.is_success:
            print(f"Output: {result.output}")

        # Or use the lower-level API methods
        store_result = await client.store_flow("workflow.yaml")
        run_result = await client.create_run(store_result.flow_id, {"x": 1})
    ```

For the low-level generated API client, use `stepflow_api` directly:
    ```python
    from stepflow_api import Client
    from stepflow_api.api.flow import store_flow
    from stepflow_api.models import StoreFlowResponse
    ```
"""

# Re-export types from stepflow_core (for StepflowExecutor protocol)
# Re-export commonly used types from stepflow_api for convenience
from stepflow_api.models import (
    CreateRunResponse,
    ExecutionStatus,
    HealthResponse,
    ItemResult,
    ItemStatistics,
    ListComponentsResponse,
    ListItemsResponse,
    ListRunsResponse,
    ListStepRunsResponse,
    RunDetails,
    StoreFlowResponse,
    WorkflowOverrides,
)
from stepflow_core import (
    ComponentInfo,
    Diagnostic,
    FlowError,
    FlowResult,
    FlowResultStatus,
    StepflowExecutor,
    ValidationResult,
)

from .client import StepflowClient, StepflowClientError

__version__ = "0.1.0"

__all__ = [
    # Client
    "StepflowClient",
    "StepflowClientError",
    # StepflowExecutor protocol types
    "StepflowExecutor",
    "FlowResult",
    "FlowResultStatus",
    "FlowError",
    "ValidationResult",
    "Diagnostic",
    "ComponentInfo",
    # API response types
    "CreateRunResponse",
    "ExecutionStatus",
    "HealthResponse",
    "ItemResult",
    "ItemStatistics",
    "ListComponentsResponse",
    "ListItemsResponse",
    "ListRunsResponse",
    "ListStepRunsResponse",
    "RunDetails",
    "StoreFlowResponse",
    "WorkflowOverrides",
]
