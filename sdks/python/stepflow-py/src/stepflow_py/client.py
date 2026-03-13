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

"""High-level async Stepflow client for workflow execution via gRPC."""

from __future__ import annotations

import json
import logging
import os
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path
from typing import TYPE_CHECKING, Any
from urllib.parse import urlparse

import grpc
from google.protobuf import json_format, struct_pb2

from stepflow_py.proto import flows_pb2, runs_pb2
from stepflow_py.proto.flows_pb2_grpc import FlowsServiceStub
from stepflow_py.proto.health_pb2_grpc import HealthServiceStub
from stepflow_py.proto.runs_pb2_grpc import RunsServiceStub

if TYPE_CHECKING:
    from stepflow_py.config import StepflowConfig

logger = logging.getLogger(__name__)


def _to_proto_value(obj: Any) -> struct_pb2.Value:
    """Convert a Python object to a google.protobuf.Value."""
    val = struct_pb2.Value()
    # Use json_format to handle the conversion properly
    json_str = json.dumps(obj)
    json_format.Parse(json_str, val)
    return val


def _to_proto_struct(obj: dict[str, Any]) -> struct_pb2.Struct:
    """Convert a Python dict to a google.protobuf.Struct."""
    s = struct_pb2.Struct()
    s.update(obj)
    return s


def _from_proto_value(val: struct_pb2.Value) -> Any:
    """Convert a google.protobuf.Value to a Python object."""
    return json_format.MessageToDict(val)


def _parse_grpc_target(url: str) -> tuple[str, bool]:
    """Parse a URL into a gRPC target and TLS flag.

    Accepts:
        - "host:port" (plain gRPC, insecure)
        - "http://host:port" (insecure)
        - "https://host:port" (TLS)
        - "http://host:port/api/v1" (strips path, insecure)

    Returns:
        (target, use_tls) where target is "host:port"
    """
    # If it looks like a plain host:port (no scheme), return as-is
    if "://" not in url:
        return url, False

    parsed = urlparse(url)
    use_tls = parsed.scheme == "https"
    host = parsed.hostname or "localhost"
    port = parsed.port or (443 if use_tls else 8080)
    return f"{host}:{port}", use_tls


class StepflowClient:
    """High-level async client for Stepflow server interactions via gRPC.

    Usage with explicit URL:
        async with StepflowClient.connect("localhost:8080") as client:
            response = await client.store_flow(workflow_dict)
            result = await client.run(response.flow_id, {"input": "value"})

    Usage with local orchestrator (requires pip install stepflow-py[local]):
        from stepflow_py.config import StepflowConfig

        config = StepflowConfig(plugins={...}, routes={...})
        async with StepflowClient.local(config) as client:
            response = await client.store_flow(workflow)
            result = await client.run(response.flow_id, input_data)
    """

    def __init__(self, channel: grpc.aio.Channel):
        """Initialize client with a gRPC channel.

        Use StepflowClient.connect() for the common case.
        """
        self._channel = channel
        self._flows_stub = FlowsServiceStub(channel)
        self._runs_stub = RunsServiceStub(channel)
        self._health_stub = HealthServiceStub(channel)

    @classmethod
    def connect(
        cls,
        target: str,
        *,
        credentials: grpc.ChannelCredentials | None = None,
        options: list[tuple[str, Any]] | None = None,
    ) -> StepflowClient:
        """Create a client connected to a Stepflow server.

        Args:
            target: Server address. Accepts:
                - "host:port" (gRPC native)
                - "http://host:port" (insecure, for backwards compatibility)
                - "https://host:port" (TLS)
            credentials: Optional gRPC channel credentials for TLS.
                If not provided and target uses https, default SSL
                credentials are used.
            options: Optional gRPC channel options.
        """
        grpc_target, use_tls = _parse_grpc_target(target)

        if credentials is not None or use_tls:
            creds = credentials or grpc.ssl_channel_credentials()
            channel = grpc.aio.secure_channel(grpc_target, creds, options=options)
        else:
            channel = grpc.aio.insecure_channel(grpc_target, options=options)

        return cls(channel)

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
        """Close the underlying gRPC channel and release resources."""
        await self._channel.close()

    async def is_healthy(self, timeout: float = 5.0) -> bool:
        """Check if server is healthy."""
        from stepflow_py.proto import health_pb2

        try:
            await self._health_stub.HealthCheck(
                health_pb2.HealthCheckRequest(),
                timeout=timeout,
            )
            return True
        except Exception:
            return False

    async def store_flow(
        self,
        flow: dict[str, Any] | Any,
        *,
        dry_run: bool = False,
        timeout: float = 30.0,
    ) -> flows_pb2.StoreFlowResponse:
        """Store a flow and return the response.

        Args:
            flow: Flow definition as a dictionary or Pydantic model with
                model_dump_json/model_dump methods.
            dry_run: If True, validate only without storing the flow
            timeout: Request timeout in seconds

        Returns:
            StoreFlowResponse with flow_id, stored flag, and diagnostics
        """
        # Support Pydantic models (e.g., stepflow_py.api.models.Flow)
        if not isinstance(flow, dict) and hasattr(flow, "model_dump"):
            flow = json.loads(flow.model_dump_json(by_alias=True))

        request = flows_pb2.StoreFlowRequest(
            flow=_to_proto_struct(flow),
            dry_run=dry_run,
        )

        return await self._flows_stub.StoreFlow(request, timeout=timeout)

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
    ) -> runs_pb2.CreateRunResponse:
        """Execute a flow and wait for the result.

        Submits the run with wait=true so the server blocks until completion
        and returns the result directly.

        Args:
            flow_id: Flow ID from store_flow()
            input_data: Single input dict or list for batch execution
            variables: Runtime variables for $variable references
            overrides: Step overrides (per step_id)
            max_concurrency: Max parallel executions for batch mode
            timeout: Client-side timeout in seconds
            wait_timeout: Server-side wait timeout in seconds (default 300).
            populate_variables_from_env: If True, fetch the flow's variable
                schema and populate variables from environment variables using
                ``env_var`` annotations. Explicit variables take priority.

        Returns:
            CreateRunResponse with summary and results
        """
        if populate_variables_from_env:
            variables = await self._merge_env_variables(flow_id, variables)

        request = self._build_create_run_request(
            flow_id=flow_id,
            input_data=input_data,
            variables=variables,
            overrides=overrides,
            max_concurrency=max_concurrency,
            wait=True,
            wait_timeout=wait_timeout,
        )

        return await self._runs_stub.CreateRun(request, timeout=timeout)

    async def submit(
        self,
        flow_id: str,
        input_data: dict[str, Any] | list[dict[str, Any]],
        variables: dict[str, Any] | None = None,
        overrides: dict[str, Any] | None = None,
        max_concurrency: int | None = None,
        timeout: float = 30.0,
        populate_variables_from_env: bool = False,
    ) -> runs_pb2.CreateRunResponse:
        """Submit a flow for execution without waiting for the result.

        Returns immediately with status Running.
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
            CreateRunResponse with summary (run_id, status)
        """
        if populate_variables_from_env:
            variables = await self._merge_env_variables(flow_id, variables)

        request = self._build_create_run_request(
            flow_id=flow_id,
            input_data=input_data,
            variables=variables,
            overrides=overrides,
            max_concurrency=max_concurrency,
            wait=False,
        )

        return await self._runs_stub.CreateRun(request, timeout=timeout)

    def _build_create_run_request(
        self,
        *,
        flow_id: str,
        input_data: dict[str, Any] | list[dict[str, Any]],
        variables: dict[str, Any] | None,
        overrides: dict[str, Any] | None,
        max_concurrency: int | None,
        wait: bool,
        wait_timeout: int | None = None,
    ) -> runs_pb2.CreateRunRequest:
        """Build a CreateRunRequest protobuf message."""
        # Normalize input to list
        inputs = [input_data] if isinstance(input_data, dict) else input_data

        request = runs_pb2.CreateRunRequest(
            flow_id=flow_id,
            input=[_to_proto_value(inp) for inp in inputs],
            wait=wait,
        )

        if variables is not None:
            for k, v in variables.items():
                request.variables[k].CopyFrom(_to_proto_value(v))

        if overrides is not None:
            request.overrides.CopyFrom(_to_proto_struct(overrides))

        if max_concurrency is not None:
            request.max_concurrency = max_concurrency

        if wait_timeout is not None:
            request.timeout_secs = wait_timeout

        return request

    async def _merge_env_variables(
        self,
        flow_id: str,
        explicit_variables: dict[str, Any] | None,
    ) -> dict[str, Any] | None:
        """Fetch flow variable schema and populate from environment.

        Uses the ``GetFlowVariables`` RPC to retrieve ``env_var`` annotations.
        Explicit variables take priority over environment values.

        Returns the merged variables dict, or None if no variables were found.
        """
        response = await self._flows_stub.GetFlowVariables(
            flows_pb2.GetFlowVariablesRequest(flow_id=flow_id)
        )

        # Build env_var mapping from proto response
        env_vars: dict[str, str] = {}
        for var_name, var_def in response.variables.items():
            if var_def.HasField("env_var"):
                env_vars[var_name] = var_def.env_var

        if not env_vars:
            logger.debug("No env_var annotations found for flow %s", flow_id)
            return explicit_variables

        logger.debug(
            "Flow %s env_var annotations: %s",
            flow_id,
            list(env_vars.items()),
        )

        env_variables: dict[str, Any] = {}
        for var_name, env_var_name in env_vars.items():
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

    async def get_flow_variables(
        self,
        flow_id: str,
        timeout: float = 30.0,
    ) -> flows_pb2.GetFlowVariablesResponse:
        """Get variable definitions for a flow.

        Returns the variable schema and environment variable mappings
        without fetching the entire flow definition.

        Args:
            flow_id: Flow ID from store_flow()
            timeout: Request timeout in seconds

        Returns:
            GetFlowVariablesResponse with flow_id and variables map
        """
        return await self._flows_stub.GetFlowVariables(
            flows_pb2.GetFlowVariablesRequest(flow_id=flow_id),
            timeout=timeout,
        )

    async def get_run(
        self,
        run_id: str,
        *,
        wait: bool = False,
        wait_timeout: int | None = None,
        timeout: float = 30.0,
    ) -> runs_pb2.GetRunResponse:
        """Get run details by ID.

        Args:
            run_id: Run ID (UUID string)
            wait: If True, long-poll until the run reaches a terminal state
            wait_timeout: Server-side wait timeout in seconds (default 300)
            timeout: Client-side timeout in seconds

        Returns:
            GetRunResponse with summary and step statuses
        """
        request = runs_pb2.GetRunRequest(run_id=run_id, wait=wait)
        if wait_timeout is not None:
            request.timeout_secs = wait_timeout
        return await self._runs_stub.GetRun(request, timeout=timeout)

    async def status_events(
        self,
        run_id: str,
        *,
        since: int | None = None,
        include_sub_runs: bool = False,
        event_types: list[runs_pb2.StatusEventType] | None = None,
        include_results: bool = False,
    ) -> AsyncIterator[runs_pb2.StatusEvent]:
        """Stream execution events for a run via gRPC server-streaming.

        Yields StatusEvent protobuf messages in journal order. The stream
        closes automatically after a RunCompletedEvent for the requested run.

        Each event has ``sequence_number`` and ``timestamp`` fields. Use
        ``sequence_number`` to resume from a specific point via ``since``.

        Args:
            run_id: Run ID (UUID string).
            since: Journal sequence number to start from (inclusive).
            include_sub_runs: Include events from sub-runs.
            event_types: Only include these event types.
            include_results: Include result payloads in completion events.

        Yields:
            StatusEvent protobuf messages.
        """
        request = runs_pb2.GetRunEventsRequest(
            run_id=run_id,
            include_sub_runs=include_sub_runs,
            include_results=include_results,
        )

        if since is not None:
            request.since = since
        if event_types:
            request.event_types.extend(event_types)  # type: ignore[arg-type]

        response_stream = self._runs_stub.GetRunEvents(request)
        async for event in response_stream:
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
        response = await self._runs_stub.GetRunItems(
            runs_pb2.GetRunItemsRequest(run_id=run_id),
            timeout=timeout,
        )

        return [
            json_format.MessageToDict(result, preserving_proto_field_name=True)
            for result in response.results
        ]


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
