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

import json
import logging
import os
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path
from typing import TYPE_CHECKING, Any, TypeAlias

from stepflow_py.api import ApiClient, Configuration
from stepflow_py.api.api import FlowApi, HealthApi, RunApi
from stepflow_py.api.models import Flow
from stepflow_py.api.models.create_run_request import CreateRunRequest
from stepflow_py.api.models.status_event import StatusEvent as _StatusEventOneOf
from stepflow_py.api.models.status_event_item_completed import StatusEventItemCompleted
from stepflow_py.api.models.status_event_run_completed import StatusEventRunCompleted
from stepflow_py.api.models.status_event_run_created import StatusEventRunCreated
from stepflow_py.api.models.status_event_run_initialized import (
    StatusEventRunInitialized,
)
from stepflow_py.api.models.status_event_step_completed import StatusEventStepCompleted
from stepflow_py.api.models.status_event_step_ready import StatusEventStepReady
from stepflow_py.api.models.status_event_step_started import StatusEventStepStarted
from stepflow_py.api.models.status_event_sub_run_created import StatusEventSubRunCreated
from stepflow_py.api.models.step_override import StepOverride
from stepflow_py.api.models.store_flow_request import StoreFlowRequest

if TYPE_CHECKING:
    from stepflow_py.api.models.create_run_response import CreateRunResponse
    from stepflow_py.api.models.run_details import RunDetails
    from stepflow_py.api.models.store_flow_response import StoreFlowResponse
    from stepflow_py.config import StepflowConfig

logger = logging.getLogger(__name__)

#: Union of all status event variant types yielded by
#: :meth:`StepflowClient.status_events`.
StatusEvent: TypeAlias = (
    StatusEventRunCreated
    | StatusEventRunInitialized
    | StatusEventStepStarted
    | StatusEventStepCompleted
    | StatusEventStepReady
    | StatusEventItemCompleted
    | StatusEventRunCompleted
    | StatusEventSubRunCreated
)


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
            config_dict = msgspec.to_builtins(config)
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
        wait_timeout: int | None = None,
        populate_variables_from_env: bool = False,
    ) -> CreateRunResponse:
        """Execute a flow and wait for the result.

        Submits the run with wait=true so the server blocks until completion
        and returns the result directly.

        Args:
            flow_id: Flow ID from store_flow()
            input_data: Single input dict or list for batch execution
            variables: Runtime variables for $variable references
            overrides: Step overrides (per step_id)
            max_concurrency: Max parallel executions for batch mode
            timeout: HTTP request timeout in seconds (client-side)
            wait_timeout: Server-side wait timeout in seconds (default 300).
                If the workflow takes longer, the server returns the current
                status rather than an error. Set this higher than ``timeout``
                is not useful since the HTTP connection will close first.
            populate_variables_from_env: If True, fetch the flow's variable
                schema and populate variables from environment variables using
                ``env_var`` annotations. Explicit variables take priority.

        Returns:
            CreateRunResponse with status and results
        """
        if populate_variables_from_env:
            variables = await self._merge_env_variables(flow_id, variables)

        # Normalize input to list (API always expects array)
        inputs = [input_data] if isinstance(input_data, dict) else input_data

        # Build request kwargs, only including values that are explicitly provided
        request_kwargs: dict[str, Any] = {
            "flowId": flow_id,
            "input": inputs,
            "wait": True,
        }
        if variables is not None:
            request_kwargs["variables"] = variables
        if overrides is not None:
            request_kwargs["overrides"] = {
                k: StepOverride.from_dict(v) for k, v in overrides.items()
            }
        if max_concurrency is not None:
            request_kwargs["maxConcurrency"] = max_concurrency
        if wait_timeout is not None:
            request_kwargs["timeoutSecs"] = wait_timeout

        request = CreateRunRequest(**request_kwargs)
        return await self._run_api.create_run(request, _request_timeout=timeout)

    async def submit(
        self,
        flow_id: str,
        input_data: dict[str, Any] | list[dict[str, Any]],
        variables: dict[str, Any] | None = None,
        overrides: dict[str, Any] | None = None,
        max_concurrency: int | None = None,
        timeout: float = 30.0,
        populate_variables_from_env: bool = False,
    ) -> CreateRunResponse:
        """Submit a flow for execution without waiting for the result.

        Returns immediately with status Running (202 Accepted).
        Use ``get_run(run_id, wait=True)`` to long-poll for completion, or
        ``get_run_items()`` to fetch results after completion.

        Args:
            flow_id: Flow ID from store_flow()
            input_data: Single input dict or list for batch execution
            variables: Runtime variables for $variable references
            overrides: Step overrides (per step_id)
            max_concurrency: Max parallel executions for batch mode
            timeout: Request timeout in seconds
            populate_variables_from_env: If True, fetch the flow's variable
                schema and populate variables from environment variables using
                ``env_var`` annotations. Explicit variables take priority.

        Returns:
            CreateRunResponse with run_id and status (typically Running)
        """
        if populate_variables_from_env:
            variables = await self._merge_env_variables(flow_id, variables)

        # Normalize input to list (API always expects array)
        inputs = [input_data] if isinstance(input_data, dict) else input_data

        request_kwargs: dict[str, Any] = {
            "flowId": flow_id,
            "input": inputs,
        }
        if variables is not None:
            request_kwargs["variables"] = variables
        if overrides is not None:
            request_kwargs["overrides"] = {
                k: StepOverride.from_dict(v) for k, v in overrides.items()
            }
        if max_concurrency is not None:
            request_kwargs["maxConcurrency"] = max_concurrency

        request = CreateRunRequest(**request_kwargs)
        return await self._run_api.create_run(request, _request_timeout=timeout)

    async def _merge_env_variables(
        self,
        flow_id: str,
        explicit_variables: dict[str, Any] | None,
    ) -> dict[str, Any] | None:
        """Fetch flow variable schema and populate from environment.

        Uses the ``GET /flows/{id}/variables`` endpoint to retrieve
        ``env_var`` annotations without fetching the entire flow.
        Explicit variables take priority over environment values.

        Returns the merged variables dict, or None if no variables were found.
        """
        response = await self._flow_api.get_flow_variables(flow_id)

        if not response.env_vars:
            logger.debug("No env_var annotations found for flow %s", flow_id)
            return explicit_variables

        logger.debug(
            "Flow %s env_var annotations: %s",
            flow_id,
            list(response.env_vars.items()),
        )

        env_variables: dict[str, Any] = {}
        for var_name, env_var_name in response.env_vars.items():
            env_value = os.environ.get(env_var_name)
            if env_value is not None:
                env_variables[var_name] = _parse_env_value(env_value)
                logger.debug(
                    "Populated variable %r from env %r",
                    var_name,
                    env_var_name,
                )
            else:
                logger.debug(
                    "Env var %r not set for variable %r",
                    env_var_name,
                    var_name,
                )

        logger.debug(
            "Merged variables for flow %s: %s (from env: %s, explicit: %s)",
            flow_id,
            list(env_variables.keys()),
            len(env_variables),
            len(explicit_variables) if explicit_variables else 0,
        )

        if not env_variables and not explicit_variables:
            return None

        # Merge: explicit variables take priority
        merged = {**env_variables}
        if explicit_variables:
            merged.update(explicit_variables)
        return merged if merged else None

    async def get_run(
        self,
        run_id: str,
        *,
        wait: bool = False,
        wait_timeout: int | None = None,
        timeout: float = 30.0,
    ) -> RunDetails:
        """Get run details by ID.

        Args:
            run_id: Run ID (UUID string)
            wait: If True, long-poll until the run reaches a terminal state
            wait_timeout: Server-side wait timeout in seconds (default 300)
            timeout: HTTP request timeout in seconds (client-side)

        Returns:
            RunDetails with status, item statistics, and item details
        """
        return await self._run_api.get_run(
            run_id,
            wait=wait if wait else None,
            timeout_secs=wait_timeout,
            _request_timeout=timeout,
        )

    async def status_events(
        self,
        run_id: str,
        *,
        since: int | None = None,
        include_sub_runs: bool = False,
        event_types: list[str] | None = None,
        include_results: bool = False,
    ) -> AsyncIterator[StatusEvent]:
        """Stream execution events for a run via Server-Sent Events.

        Yields typed status event objects (e.g., ``StatusEventRunCreated``,
        ``StatusEventStepCompleted``) in journal order. The stream closes
        automatically after a ``StatusEventRunCompleted`` for the requested
        run.

        Each event has ``sequence_number`` and ``timestamp`` fields. Use
        ``sequence_number`` to resume from a specific point via ``since``.

        Args:
            run_id: Run ID (UUID string).
            since: Journal sequence number to start from (inclusive).
            include_sub_runs: Include events from sub-runs.
            event_types: Only include these event types (e.g.,
                ``["step_started", "step_completed"]``).
            include_results: Include result payloads in completion events.

        Yields:
            A typed status event variant for each execution event.
        """
        import httpx

        params: dict[str, str] = {}
        if since is not None:
            params["since"] = str(since)
        if include_sub_runs:
            params["includeSubRuns"] = "true"
        if event_types:
            params["eventTypes"] = ",".join(event_types)
        if include_results:
            params["includeResults"] = "true"

        host: str = str(self._api_client.configuration.host)
        url = f"{host}/runs/{run_id}/events"

        # Reuse default headers (auth, user-agent, etc.) from the API client
        headers = dict(self._api_client.default_headers)
        headers["Accept"] = "text/event-stream"

        async with httpx.AsyncClient(headers=headers) as http_client:
            async with http_client.stream(
                "GET", url, params=params, timeout=None
            ) as response:
                response.raise_for_status()
                async for event in _parse_sse_stream(response.aiter_lines()):
                    yield event

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


async def _parse_sse_stream(
    lines: AsyncIterator[str],
) -> AsyncIterator[StatusEvent]:
    """Parse an SSE byte stream into typed status event objects.

    Implements the subset of the SSE spec needed for Stepflow:
    ``id``, ``event``, and ``data`` fields. Ignores comments
    (lines starting with ``:``).

    The ``data`` payload is deserialized into the appropriate generated
    status event variant (e.g., ``StatusEventRunCreated``) using the
    ``event`` discriminator field.
    """
    data_buf: list[str] = []

    async for line in lines:
        if line.startswith(":"):
            continue

        if line == "":
            # Empty line = event boundary
            if data_buf:
                raw = "\n".join(data_buf)
                event = _deserialize_status_event(raw)
                if event is not None:
                    yield event
            data_buf = []
            continue

        if ":" in line:
            field, _, value = line.partition(":")
            value = value.lstrip(" ")  # SSE spec: strip single leading space
        else:
            field = line
            value = ""

        if field == "data":
            data_buf.append(value)

    # Flush any trailing event (stream closed without final blank line)
    if data_buf:
        raw = "\n".join(data_buf)
        event = _deserialize_status_event(raw)
        if event is not None:
            yield event


def _deserialize_status_event(json_str: str) -> StatusEvent | None:
    """Deserialize a JSON string into a typed status event variant.

    Returns ``None`` if the JSON cannot be parsed into a known variant.
    """
    try:
        wrapper = _StatusEventOneOf.from_json(json_str)
        return wrapper.actual_instance
    except (ValueError, Exception):
        logger.debug("Failed to deserialize status event: %s", json_str[:200])
        return None



def _parse_env_value(value: str) -> Any:
    """Parse an environment variable value, trying JSON first.

    This matches the Rust CLI behavior: if the value is valid JSON
    (number, boolean, object, array, null), use the parsed value.
    Otherwise treat it as a plain string.
    """
    try:
        return json.loads(value)
    except (json.JSONDecodeError, ValueError):
        return value
