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

"""High-level async Stepflow client for workflow execution."""

from __future__ import annotations

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path
from typing import TYPE_CHECKING, Any

from stepflow_py.api import ApiClient, Configuration
from stepflow_py.api.api import FlowApi, HealthApi, RunApi
from stepflow_py.api.models import Flow
from stepflow_py.api.models.create_run_request import CreateRunRequest
from stepflow_py.api.models.store_flow_request import StoreFlowRequest
from stepflow_py.api.models.workflow_overrides import WorkflowOverrides

if TYPE_CHECKING:
    from stepflow_py.api.models.create_run_response import CreateRunResponse
    from stepflow_py.api.models.store_flow_response import StoreFlowResponse
    from stepflow_py.config import StepflowConfig


class StepflowClient:
    """High-level async client for Stepflow server interactions.

    Usage with explicit URL:
        async with StepflowClient.connect("http://localhost:8080") as client:
            response = await client.store_flow(workflow_dict)
            result = await client.run(response.flow_id, {"input": "value"})

    Usage with local orchestrator (requires pip install stepflow-py[local]):
        from stepflow_py.config import StepflowConfig

        config = StepflowConfig(plugins={...}, routes={...})
        async with StepflowClient.local(config) as client:
            response = await client.store_flow(workflow)
            result = await client.run(response.flow_id, input_data)
    """

    def __init__(self, api_client: ApiClient):
        """Initialize client with an ApiClient instance.

        Use StepflowClient.connect() for the common case.
        """
        self._api_client = api_client
        self._flow_api = FlowApi(api_client)
        self._run_api = RunApi(api_client)
        self._health_api = HealthApi(api_client)

    @classmethod
    def connect(cls, base_url: str) -> StepflowClient:
        """Create a client connected to a Stepflow server.

        Args:
            base_url: Server URL (e.g., "http://localhost:8080")
                      Will automatically append /api/v1 if not present.
        """
        if not base_url.endswith("/api/v1"):
            base_url = f"{base_url.rstrip('/')}/api/v1"
        config = Configuration(host=base_url)
        api_client = ApiClient(configuration=config)
        return cls(api_client)

    @classmethod
    @asynccontextmanager
    async def local(
        cls,
        config: StepflowConfig | Path,
        *,
        startup_timeout: float = 30.0,
        shutdown_timeout: float = 10.0,
        log_level: str = "info",
        env: dict[str, str] | None = None,
    ) -> AsyncIterator[StepflowClient]:
        """Start a local orchestrator and return a connected client.

        The client owns the orchestrator and shuts it down when the context exits.

        Args:
            config: StepflowConfig object or Path to config file
            startup_timeout: Max seconds to wait for orchestrator startup
            shutdown_timeout: Max seconds to wait for graceful shutdown
            log_level: Orchestrator log level (debug, info, warn, error)
            env: Additional environment variables for the orchestrator process

        Yields:
            StepflowClient connected to the local orchestrator.

        Raises:
            ImportError: If stepflow-orchestrator is not installed.
                Install with: pip install stepflow-py[local]

        Example:
            from stepflow_py import StepflowClient
            from stepflow_py.config import StepflowConfig

            config = StepflowConfig(plugins={...}, routes={...})
            async with StepflowClient.local(config) as client:
                response = await client.store_flow(workflow)
                result = await client.run(response.flow_id, input_data)

        For separate orchestrator lifecycle management, use StepflowClient.connect()
        with a manually-managed StepflowOrchestrator instead.
        """
        try:
            from stepflow_orchestrator import OrchestratorConfig, StepflowOrchestrator
        except ImportError as e:
            raise ImportError(
                "stepflow-orchestrator is required for local(). "
                "Install with: pip install stepflow-py[local]"
            ) from e

        import msgspec

        from stepflow_py.config import StepflowConfig as StepflowConfigType

        # Build orchestrator config based on input type
        if isinstance(config, Path):
            orch_config = OrchestratorConfig(
                config_path=config,
                startup_timeout=startup_timeout,
                shutdown_timeout=shutdown_timeout,
                log_level=log_level,
                env=env or {},
            )
        elif isinstance(config, StepflowConfigType):
            # Convert StepflowConfig to dict, removing None values
            config_dict = msgspec.to_builtins(config)
            config_dict = _remove_none_values(config_dict)
            orch_config = OrchestratorConfig(
                config=config_dict,
                startup_timeout=startup_timeout,
                shutdown_timeout=shutdown_timeout,
                log_level=log_level,
                env=env or {},
            )
        else:
            raise TypeError(
                f"config must be StepflowConfig or Path, got {type(config).__name__}"
            )

        # Start orchestrator - client owns it, shuts down on exit
        async with StepflowOrchestrator.start(orch_config) as orchestrator:
            client = cls.connect(orchestrator.url)
            try:
                yield client
            finally:
                await client.close()

    async def __aenter__(self) -> StepflowClient:
        return self

    async def __aexit__(self, *args: object) -> None:
        await self.close()

    async def close(self) -> None:
        """Close the underlying API client and release resources."""
        await self._api_client.close()

    @property
    def base_url(self) -> str:
        """Return the base URL of the connected server."""
        host: str = str(self._api_client.configuration.host)
        # Remove /api/v1 suffix to get base URL
        if host.endswith("/api/v1"):
            return host[:-7]
        return host

    async def is_healthy(self, timeout: float = 5.0) -> bool:
        """Check if server is healthy."""
        try:
            await self._health_api.health_check(_request_timeout=timeout)
            return True
        except Exception:
            return False

    async def store_flow(
        self,
        flow: Flow | dict[str, Any],
        *,
        dry_run: bool = False,
        timeout: float = 30.0,
    ) -> StoreFlowResponse:
        """Store a flow and return the response.

        Args:
            flow: Flow object or flow definition as a dictionary
            dry_run: If True, validate only without storing the flow
            timeout: Request timeout in seconds

        Returns:
            StoreFlowResponse with flow_id (None if dry_run) and diagnostics
        """
        if isinstance(flow, Flow):
            flow_model = flow
        else:
            parsed = Flow.from_dict(flow)
            if parsed is None:
                raise ValueError("Failed to parse flow dictionary")
            flow_model = parsed
        request = StoreFlowRequest(flow=flow_model, dry_run=dry_run)
        return await self._flow_api.store_flow(request, _request_timeout=timeout)

    async def run(
        self,
        flow_id: str,
        input_data: dict[str, Any] | list[dict[str, Any]],
        variables: dict[str, Any] | None = None,
        overrides: dict[str, Any] | None = None,
        max_concurrency: int | None = None,
        timeout: float = 300.0,
    ) -> CreateRunResponse:
        """Execute a flow and return the result.

        Args:
            flow_id: Flow ID from store_flow()
            input_data: Single input dict or list for batch execution
            variables: Runtime variables for $variable references
            overrides: Step overrides (per step_id)
            max_concurrency: Max parallel executions for batch mode
            timeout: Request timeout in seconds

        Returns:
            CreateRunResponse with status and result
        """
        # Normalize input to list (API always expects array)
        inputs = [input_data] if isinstance(input_data, dict) else input_data

        # Build request kwargs, only including non-None values
        # This ensures exclude_unset=True works correctly (unset != set to None)
        request_kwargs: dict[str, Any] = {
            "flowId": flow_id,
            "input": inputs,
        }
        if variables is not None:
            request_kwargs["variables"] = variables
        if overrides is not None:
            request_kwargs["overrides"] = WorkflowOverrides.from_dict(overrides)
        if max_concurrency is not None:
            request_kwargs["maxConcurrency"] = max_concurrency

        request = CreateRunRequest(**request_kwargs)
        return await self._run_api.create_run(request, _request_timeout=timeout)

    async def get_run_items(
        self, run_id: str, timeout: float = 30.0
    ) -> list[dict[str, Any]]:
        """Get items for a batch run.

        Args:
            run_id: Run ID from a batch execution
            timeout: Request timeout in seconds

        Returns:
            List of item results as dictionaries
        """
        response = await self._run_api.get_run_items(run_id, _request_timeout=timeout)
        return [
            item.model_dump(by_alias=True, exclude_unset=True)
            for item in response.items
        ]


def _remove_none_values(obj: Any) -> Any:
    """Recursively remove None values from a dict or list.

    This is needed because Rust's serde untagged enum deserialization
    can fail when explicit null values are present for optional fields.
    """
    if isinstance(obj, dict):
        return {k: _remove_none_values(v) for k, v in obj.items() if v is not None}
    elif isinstance(obj, list):
        return [_remove_none_values(item) for item in obj]
    return obj
