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

"""Pull-based gRPC worker for Stepflow.

Connects to any orchestrator's TasksService.PullTasks, receives task
assignments, executes components via StepflowServer, and reports
completion via OrchestratorService.CompleteTask on the run-owning
orchestrator.

TasksService is an orchestrator-hosted service — the worker is a
*client* of this service, not its host. The worker calls PullTasks to
receive work, then uses the orchestrator_service_url from each task's
TaskContext to report completion.

Static worker configuration (blob service URL, blobification threshold)
is read from environment variables at startup:
  - STEPFLOW_BLOB_URL: URL for the blob gRPC/HTTP API (e.g., "localhost:7837")
  - STEPFLOW_BLOB_THRESHOLD_BYTES: Byte threshold for auto-blobification
    (default: 0 = disabled)

Task lifecycle:
1. Receive TaskAssignment from PullTasks stream
2. Call StartTask — if timed_out=true, skip execution
3. Start background heartbeat loop (every heartbeat_interval_secs)
4. Execute component
5. Call CompleteTask with result or error
6. Cancel heartbeat loop
"""

from __future__ import annotations

import asyncio
import inspect
import logging
import math
import os
import traceback
from typing import TYPE_CHECKING, Any

import grpc
import grpc.aio
from google.protobuf import struct_pb2

from stepflow_py.proto import (
    CompleteTaskRequest,
    ComponentExecuteResponse,
    ComponentInfo,
    PullTasksRequest,
    StartTaskRequest,
    TaskAssignment,
    TaskError,
    TaskHeartbeatRequest,
)
from stepflow_py.proto.common_pb2 import (
    TASK_ERROR_CODE_COMPONENT_FAILED,
    TASK_ERROR_CODE_INVALID_INPUT,
    TASK_ERROR_CODE_RESOURCE_UNAVAILABLE,
    TASK_ERROR_CODE_WORKER_ERROR,
)
from stepflow_py.proto.orchestrator_pb2_grpc import OrchestratorServiceStub
from stepflow_py.proto.tasks_pb2_grpc import TasksServiceStub

if TYPE_CHECKING:
    from stepflow_py.worker.server import ComponentEntry, StepflowServer

logger = logging.getLogger(__name__)

# Static worker configuration from environment variables.
# Read once at module import; does not change during worker lifetime.
_BLOB_URL: str = os.environ.get("STEPFLOW_BLOB_URL", "")
_BLOB_THRESHOLD_BYTES: int = int(os.environ.get("STEPFLOW_BLOB_THRESHOLD_BYTES", "0"))

# Queue name set at runtime by run_grpc_worker(); used as metric attribute.
_QUEUE_NAME: str = ""

# --- OpenTelemetry tracing imports (optional dependency) ---
try:
    from opentelemetry import trace as otel_trace

    from stepflow_py.worker.observability import (
        extract_trace_context,
        set_diagnostic_context,
    )

    _OTEL_TRACE_AVAILABLE = True
except ImportError:
    _OTEL_TRACE_AVAILABLE = False

# --- OpenTelemetry metrics instruments (optional dependency) ---
# Instruments are lazily created on first use because grpc_worker.py is
# imported before setup_observability() installs a real MeterProvider.
# Creating instruments from the proxy meter before the provider is set
# yields permanently no-op counters in the Python OTel SDK.
_OTEL_METRICS_AVAILABLE = False
_tasks_pulled: Any = None
_tasks_completed: Any = None
_heartbeats_sent: Any = None
_connection_status: Any = None
_metrics_initialized = False

try:
    from opentelemetry import metrics as otel_metrics

    _OTEL_METRICS_AVAILABLE = True
except ImportError:
    pass


def _ensure_metrics() -> None:
    """Lazily create OTel metric instruments after MeterProvider is set."""
    global _tasks_pulled, _tasks_completed, _heartbeats_sent  # noqa: PLW0603
    global _connection_status, _metrics_initialized  # noqa: PLW0603
    if _metrics_initialized or not _OTEL_METRICS_AVAILABLE:
        return
    _metrics_initialized = True
    try:
        meter = otel_metrics.get_meter("stepflow-grpc-worker")
        _tasks_pulled = meter.create_counter(
            "worker.tasks_pulled_total",
            description="Total tasks pulled from the queue",
        )
        _tasks_completed = meter.create_counter(
            "worker.tasks_completed_total",
            description="Total tasks completed (success or error)",
        )
        _heartbeats_sent = meter.create_counter(
            "worker.heartbeats_sent_total",
            description="Total heartbeats sent to the orchestrator",
        )
        _connection_status = meter.create_up_down_counter(
            "worker.connection_status",
            description="Number of active pull connections (0 or 1)",
        )
    except Exception:
        pass


async def run_grpc_worker(
    server: StepflowServer,
    tasks_url: str,
    queue_name: str,
    max_concurrent: int = 4,
    max_retries: int = 15,
) -> None:
    """Run the gRPC pull-based worker.

    Connects to the orchestrator's TasksService.PullTasks, receives task
    assignments, executes components, and reports results via
    OrchestratorService.CompleteTask on the run-owning orchestrator.

    Static configuration (blob URL, blob threshold) is read from
    environment variables at startup — see module docstring.

    Args:
        server: StepflowServer with registered components.
        tasks_url: URL of any orchestrator hosting TasksService.
        queue_name: Queue name from orchestrator config (e.g., "python").
        max_concurrent: Maximum concurrent task executions.
        max_retries: Maximum consecutive connection failures before giving up.
    """
    global _QUEUE_NAME  # noqa: PLW0603
    _QUEUE_NAME = queue_name
    _ensure_metrics()

    logger.info(
        "Starting gRPC worker: tasks_url=%s, queue=%s, max_concurrent=%d, "
        "blob_url=%s, blob_threshold=%d",
        tasks_url,
        queue_name,
        max_concurrent,
        _BLOB_URL or "(not configured)",
        _BLOB_THRESHOLD_BYTES,
    )

    # Build component info list from registered components
    components = _build_component_info_list(server)
    if not components:
        logger.error("No components registered — nothing to serve")
        return

    logger.info(
        "Registered %d components: %s",
        len(components),
        ", ".join(c.name for c in components),
    )

    # Semaphore for concurrency control
    semaphore = asyncio.Semaphore(max_concurrent)

    consecutive_failures = 0
    had_successful_connection = False
    while True:
        try:
            await _pull_loop(
                server=server,
                tasks_url=tasks_url,
                queue_name=queue_name,
                components=components,
                semaphore=semaphore,
                max_concurrent=max_concurrent,
            )
            # Successful pull session — reset failure counter
            consecutive_failures = 0
            had_successful_connection = True
        except grpc.aio.AioRpcError as e:
            consecutive_failures += 1

            # If we previously had a successful connection and the server
            # is now gone (UNAVAILABLE), exit cleanly. This happens when
            # the orchestrator shuts down — no point retrying.
            if had_successful_connection and e.code() == grpc.StatusCode.UNAVAILABLE:
                logger.info(
                    "gRPC server went away after successful session "
                    "(code=%s). Shutting down.",
                    e.code(),
                )
                return

            if consecutive_failures >= max_retries:
                logger.error(
                    "gRPC worker giving up after %d consecutive failures "
                    "(last: code=%s, %s)",
                    consecutive_failures,
                    e.code(),
                    e.details(),
                )
                return
            logger.warning(
                "gRPC connection lost (code=%s): %s. Reconnecting in 2s... "
                "(attempt %d/%d)",
                e.code(),
                e.details(),
                consecutive_failures,
                max_retries,
            )
            await asyncio.sleep(2)
        except Exception:
            consecutive_failures += 1
            if consecutive_failures >= max_retries:
                logger.error(
                    "gRPC worker giving up after %d consecutive failures",
                    consecutive_failures,
                )
                return
            logger.exception(
                "Unexpected error in pull loop. Reconnecting in 5s... (attempt %d/%d)",
                consecutive_failures,
                max_retries,
            )
            await asyncio.sleep(5)


async def _pull_loop(
    server: StepflowServer,
    tasks_url: str,
    queue_name: str,
    components: list[ComponentInfo],
    semaphore: asyncio.Semaphore,
    max_concurrent: int,
) -> None:
    """Single pull session — connects, pulls tasks, processes them."""
    channel = grpc.aio.insecure_channel(tasks_url)
    if _connection_status is not None:
        _connection_status.add(1, {"queue_name": _QUEUE_NAME})
    try:
        tasks_stub = TasksServiceStub(channel)

        request = PullTasksRequest(
            queue_name=queue_name,
            max_concurrent=max_concurrent,
            components=components,
        )

        logger.info("Connecting to TasksService at %s...", tasks_url)
        stream = tasks_stub.PullTasks(request)

        async for task in stream:
            if _tasks_pulled is not None:
                _tasks_pulled.add(1, {"queue_name": _QUEUE_NAME})
            await semaphore.acquire()
            asyncio.create_task(
                _handle_task(server, task, semaphore),
            )
    finally:
        if _connection_status is not None:
            _connection_status.add(-1, {"queue_name": _QUEUE_NAME})
        await channel.close()


async def _handle_task(
    server: StepflowServer,
    task: TaskAssignment,
    semaphore: asyncio.Semaphore,
) -> None:
    """Handle a single task: StartTask, heartbeat, execute, CompleteTask."""
    orchestrator_url = (
        task.request.context.orchestrator_service_url
        if task.request.HasField("context")
        else ""
    )

    heartbeat_task: asyncio.Task[None] | None = None
    orch_channel: grpc.aio.Channel | None = None

    try:
        req = task.request
        logger.info(
            "Executing task %s: component=%s, attempt=%d",
            task.task_id,
            req.component,
            req.attempt,
        )

        # Set up observability context
        obs = req.observability
        _set_execution_context(
            run_id=obs.run_id if obs.HasField("run_id") else None,
            step_id=obs.step_id if obs.HasField("step_id") else None,
            flow_id=obs.flow_id if obs.HasField("flow_id") else None,
            attempt=req.attempt,
        )

        # Extract parent trace context from orchestrator for distributed tracing
        parent_ctx = None
        if _OTEL_TRACE_AVAILABLE:
            try:
                trace_id = obs.trace_id if obs.HasField("trace_id") else None
                span_id = obs.span_id if obs.HasField("span_id") else None
                span_context = extract_trace_context(trace_id, span_id)
                if span_context is not None:
                    parent_ctx = otel_trace.set_span_in_context(
                        otel_trace.NonRecordingSpan(span_context)
                    )
            except Exception:
                logger.debug("Failed to extract trace context", exc_info=True)

        # Build span wrapper for distributed tracing
        _span_cm = None
        if _OTEL_TRACE_AVAILABLE and parent_ctx is not None:
            try:
                tracer = otel_trace.get_tracer("stepflow-grpc-worker")
                _span_cm = tracer.start_as_current_span(
                    f"execute {req.component}",
                    context=parent_ctx,
                    attributes={
                        "stepflow.task_id": task.task_id,
                        "stepflow.component": req.component,
                        "stepflow.attempt": req.attempt,
                        "stepflow.queue_name": _QUEUE_NAME,
                    },
                )
            except Exception:
                logger.debug("Failed to create trace span", exc_info=True)

        # Use the span context manager if available, otherwise a no-op
        from contextlib import nullcontext

        span_ctx = _span_cm if _span_cm is not None else nullcontext()

        # Open a shared channel to the run-owning orchestrator for
        # StartTask, heartbeats, and CompleteTask
        if orchestrator_url:
            orch_channel = grpc.aio.insecure_channel(orchestrator_url)
            stub = OrchestratorServiceStub(orch_channel)

            # Call StartTask — if timed_out, skip execution
            try:
                response = await stub.StartTask(StartTaskRequest(task_id=task.task_id))
                if response.timed_out:
                    logger.warning(
                        "Task %s timed out in queue, skipping execution",
                        task.task_id,
                    )
                    return
            except grpc.aio.AioRpcError as e:
                logger.error(
                    "StartTask failed for task %s: %s (code=%s)",
                    task.task_id,
                    e.details(),
                    e.code(),
                )
                # If StartTask fails (e.g., task already timed out and
                # removed), skip execution
                return

            # Start background heartbeat loop
            interval = task.heartbeat_interval_secs or 1
            heartbeat_task = asyncio.create_task(
                _heartbeat_loop(stub, task.task_id, interval)
            )

        # Convert proto Value to Python dict
        input_data = _proto_value_to_python(req.input)

        # Look up the component — strip leading slash since the orchestrator
        # sends the resolved path (e.g., "/echo") but components register
        # by name (e.g., "echo").
        component_name = req.component.lstrip("/")
        lookup = server.get_component(component_name)
        if lookup is None:
            # Try with the original path in case of pattern-based registration
            lookup = server.get_component(req.component)
        if lookup is None:
            await _complete_task_error(
                task,
                f"Component '{req.component}' not found",
                channel=orch_channel,
            )
            return

        component, path_params = lookup

        # Span wraps only component execution, not StartTask/heartbeat setup
        with span_ctx:
            # Set diagnostic context inside span so trace/span IDs are captured
            if _OTEL_TRACE_AVAILABLE:
                try:
                    set_diagnostic_context(
                        flow_id=obs.flow_id if obs.HasField("flow_id") else None,
                        run_id=obs.run_id if obs.HasField("run_id") else None,
                        step_id=obs.step_id if obs.HasField("step_id") else None,
                    )
                except Exception:
                    pass

            # Execute the component
            try:
                output = await _execute_component(
                    server, component, input_data, req, path_params
                )
                # Convert output to proto Value
                proto_output = _python_to_proto_value(output)
                await _complete_task_success(
                    task,
                    proto_output,
                    channel=orch_channel,
                )
            except Exception as e:
                logger.error("Component %s failed: %s", req.component, e, exc_info=True)
                error_code, error_data = _classify_exception(e)
                await _complete_task_error(
                    task,
                    str(e),
                    code=error_code,
                    data=error_data,
                    channel=orch_channel,
                )

    except Exception:
        logger.exception("Failed to handle task %s", task.task_id)
        try:
            await _complete_task_error(
                task,
                traceback.format_exc(),
                code=TASK_ERROR_CODE_WORKER_ERROR,
                channel=orch_channel,
            )
        except Exception:
            logger.exception("Failed to report error for task %s", task.task_id)
    finally:
        if heartbeat_task:
            heartbeat_task.cancel()
            try:
                await heartbeat_task
            except asyncio.CancelledError:
                pass
        if orch_channel:
            await orch_channel.close()
        semaphore.release()


async def _heartbeat_loop(
    stub: Any,  # OrchestratorServiceStub (async variant)
    task_id: str,
    interval_secs: int,
) -> None:
    """Send periodic heartbeats until cancelled."""
    try:
        while True:
            await asyncio.sleep(interval_secs)
            try:
                response = await stub.TaskHeartbeat(
                    TaskHeartbeatRequest(task_id=task_id)
                )
                if _heartbeats_sent is not None:
                    _heartbeats_sent.add(1, {"queue_name": _QUEUE_NAME})
                if response.should_cancel:
                    logger.info("Task %s cancellation requested", task_id)
                    break
            except grpc.aio.AioRpcError as e:
                logger.warning(
                    "Heartbeat failed for task %s: %s (code=%s)",
                    task_id,
                    e.details(),
                    e.code(),
                )
                break
    except asyncio.CancelledError:
        pass


async def _execute_component(
    server: StepflowServer,
    component: ComponentEntry,
    input_data: Any,
    req: Any,
    path_params: dict[str, str],
) -> Any:
    """Execute a component function and return its output."""
    import msgspec

    # Parse input using component's input type
    try:
        input_value: Any = msgspec.convert(input_data, type=component.input_type)
    except TypeError as type_err:
        if "is not supported" in str(type_err):
            input_value = input_data
        else:
            raise

    # Resolve blob refs if threshold is set (from env var or server config)
    if _BLOB_THRESHOLD_BYTES > 0 or server._blob_threshold > 0:  # noqa: SLF001
        from stepflow_py.worker.blob_ref import resolve_blob_refs

        input_value = await resolve_blob_refs(input_value)

    # Build args — context is created from TaskContext in the task
    args = [input_value]

    # Check if component needs context
    sig = inspect.signature(component.function)
    has_context_param = any(
        p.annotation is not inspect.Parameter.empty and _is_context_type(p.annotation)
        for p in sig.parameters.values()
        if p.name != "input" and p.name not in path_params
    )

    if has_context_param:
        from stepflow_py.worker.grpc_context import GrpcContext

        context = GrpcContext(
            orchestrator_url=req.context.orchestrator_service_url
            if req.HasField("context")
            else "",
            blob_url=_BLOB_URL,
            blob_threshold=_BLOB_THRESHOLD_BYTES,
            run_id=req.observability.run_id
            if req.observability.HasField("run_id")
            else None,
            flow_id=req.observability.flow_id
            if req.observability.HasField("flow_id")
            else None,
            step_id=req.observability.step_id
            if req.observability.HasField("step_id")
            else None,
            attempt=req.attempt,
        )
        args.append(context)

    if inspect.iscoroutinefunction(component.function):
        output = await component.function(*args, **path_params)
    else:
        output = component.function(*args, **path_params)

    # Convert output to JSON-serializable form
    if hasattr(output, "__struct_fields__"):
        # msgspec Struct → dict
        output = msgspec.to_builtins(output)
    elif hasattr(output, "model_dump"):
        # Pydantic → dict
        output = output.model_dump()

    return output


def _is_context_type(annotation: Any) -> bool:
    """Check if a parameter annotation is a StepflowContext type."""
    from stepflow_py.worker.context import StepflowContext

    try:
        if annotation is StepflowContext:
            return True
        if isinstance(annotation, str) and "Context" in annotation:
            return True
    except Exception:
        pass
    return False


def _classify_exception(exc: Exception) -> tuple[int, dict | None]:
    """Map a Python exception to a proto TaskErrorCode and optional structured data.

    Returns (error_code, error_data) where error_code is a TaskErrorCode int
    and error_data is an optional dict with structured error context.

    StepflowError subclasses define their own task_error_code and
    task_error_data(); standard Python exceptions are mapped here.
    """
    from stepflow_py.worker.exceptions import StepflowError

    # StepflowError hierarchy is self-describing
    if isinstance(exc, StepflowError):
        error_data = exc.task_error_data()
        tb = traceback.format_exception(type(exc), exc, exc.__traceback__)
        if tb:
            error_data["traceback"] = "".join(tb)
        return exc.task_error_code, error_data

    # Standard Python exceptions
    std_error_data: dict = {"exception_type": type(exc).__name__}
    tb = traceback.format_exception(type(exc), exc, exc.__traceback__)
    if tb:
        std_error_data["traceback"] = "".join(tb)

    if isinstance(exc, ValueError):
        return TASK_ERROR_CODE_COMPONENT_FAILED, std_error_data
    if isinstance(exc, TypeError):
        return TASK_ERROR_CODE_INVALID_INPUT, std_error_data
    if isinstance(exc, ConnectionError | TimeoutError | OSError):
        return TASK_ERROR_CODE_RESOURCE_UNAVAILABLE, std_error_data

    # Default: component failed
    return TASK_ERROR_CODE_COMPONENT_FAILED, std_error_data


async def _complete_task_with_retry(
    orchestrator_url: str,
    request: CompleteTaskRequest,
    task_id: str,
    *,
    max_retries: int = 5,
    base_delay: float = 2.0,
) -> bool:
    """Send CompleteTask with retry on transient failures.

    Retries on:
    - UNAVAILABLE: orchestrator is down (restarting). Reconnects channel.
    - NOT_FOUND: orchestrator is up but hasn't re-registered this task_id
      yet (recovery in progress). Retries until the task is re-registered.

    Returns True if the result was delivered, False if all retries failed.
    """
    channel = grpc.aio.insecure_channel(orchestrator_url)
    try:
        stub = OrchestratorServiceStub(channel)
        for attempt in range(max_retries + 1):
            try:
                await stub.CompleteTask(request)
                return True
            except grpc.aio.AioRpcError as e:
                retriable = e.code() in (
                    grpc.StatusCode.UNAVAILABLE,
                    grpc.StatusCode.NOT_FOUND,
                )
                if retriable and attempt < max_retries:
                    delay = min(base_delay * (2**attempt), 30.0)
                    logger.warning(
                        "CompleteTask %s for %s, retry in %.1fs (%d/%d)",
                        e.code(),
                        task_id,
                        delay,
                        attempt + 1,
                        max_retries,
                    )
                    await asyncio.sleep(delay)
                    # Reconnect on UNAVAILABLE (server may have restarted)
                    if e.code() == grpc.StatusCode.UNAVAILABLE:
                        await channel.close()
                        channel = grpc.aio.insecure_channel(orchestrator_url)
                        stub = OrchestratorServiceStub(channel)
                else:
                    logger.error(
                        "CompleteTask failed for %s: %s (code=%s)",
                        task_id,
                        e.details(),
                        e.code(),
                    )
                    return False
    finally:
        await channel.close()
    return False


async def _complete_task_success(
    task: TaskAssignment,
    output: struct_pb2.Value,
    *,
    channel: grpc.aio.Channel | None = None,
) -> None:
    """Report successful task completion to the run-owning orchestrator."""
    orchestrator_url = (
        task.request.context.orchestrator_service_url
        if task.request.HasField("context")
        else ""
    )
    if not orchestrator_url:
        logger.error(
            "No orchestrator_service_url for task %s — cannot report completion",
            task.task_id,
        )
        return

    request = CompleteTaskRequest(
        task_id=task.task_id,
        response=ComponentExecuteResponse(output=output),
    )
    if await _complete_task_with_retry(orchestrator_url, request, task.task_id):
        logger.debug("Completed task %s (success)", task.task_id)
        if _tasks_completed is not None:
            _tasks_completed.add(1, {"queue_name": _QUEUE_NAME, "outcome": "success"})

    # Close the shared channel if one was passed in (caller manages lifecycle)
    if channel is not None:
        await channel.close()


async def _complete_task_error(
    task: TaskAssignment,
    error_msg: str,
    *,
    code: int = TASK_ERROR_CODE_COMPONENT_FAILED,
    data: dict | None = None,
    channel: grpc.aio.Channel | None = None,
) -> None:
    """Report task failure to the run-owning orchestrator."""
    orchestrator_url = (
        task.request.context.orchestrator_service_url
        if task.request.HasField("context")
        else ""
    )
    if not orchestrator_url:
        logger.error(
            "No orchestrator_service_url for task %s — cannot report error",
            task.task_id,
        )
        return

    task_error = TaskError(
        code=code,  # type: ignore[arg-type]
        message=error_msg,
    )
    # Attach structured error data if provided
    if data:
        proto_data = struct_pb2.Struct()
        for k, v in data.items():
            proto_data.fields[str(k)].CopyFrom(_python_to_proto_value(v))
        task_error.data.CopyFrom(proto_data)
    request = CompleteTaskRequest(
        task_id=task.task_id,
        error=task_error,
    )
    if await _complete_task_with_retry(orchestrator_url, request, task.task_id):
        logger.debug("Completed task %s (error, code=%s)", task.task_id, code)
        if _tasks_completed is not None:
            _tasks_completed.add(1, {"queue_name": _QUEUE_NAME, "outcome": "error"})

    # Close the shared channel if one was passed in (caller manages lifecycle)
    if channel is not None:
        await channel.close()


def _build_component_info_list(
    server: StepflowServer,
) -> list[ComponentInfo]:
    """Build proto ComponentInfo list from registered components."""
    import json

    infos = []
    for name, entry in server._components.items():  # noqa: SLF001
        info = ComponentInfo(
            name=name,
            description=entry.description or "",
        )
        try:
            info.input_schema = json.dumps(entry.input_schema())
        except Exception:
            pass
        try:
            info.output_schema = json.dumps(entry.output_schema())
        except Exception:
            pass
        infos.append(info)
    return infos


def _set_execution_context(
    run_id: str | None,
    step_id: str | None,
    flow_id: str | None,
    attempt: int,
) -> None:
    """Set execution context vars for the current task."""
    try:
        from stepflow_py.worker import execution_context

        execution_context.set_context(
            run_id=run_id,
            step_id=step_id,
            flow_id=flow_id,
            attempt=attempt,
        )
    except Exception:
        pass


def _proto_value_to_python(value: struct_pb2.Value) -> Any:
    """Convert a protobuf Value to a Python object.

    Preserves integer types: protobuf number_value is always f64, so
    whole-number floats (e.g. 30.0) are converted back to int (30) to
    maintain parity with JSON-RPC transport.
    """
    kind = value.WhichOneof("kind")
    if kind == "null_value":
        return None
    elif kind == "bool_value":
        return value.bool_value
    elif kind == "number_value":
        n = value.number_value
        if n.is_integer() and math.isfinite(n):
            return int(n)
        return n
    elif kind == "string_value":
        return value.string_value
    elif kind == "struct_value":
        return {
            k: _proto_value_to_python(v) for k, v in value.struct_value.fields.items()
        }
    elif kind == "list_value":
        return [_proto_value_to_python(v) for v in value.list_value.values]
    else:
        return None


def _python_to_proto_value(obj: Any) -> struct_pb2.Value:
    """Convert a Python object to a protobuf Value.

    Handles dicts with non-string keys by converting them to strings,
    since protobuf Struct only supports string keys.
    """
    value = struct_pb2.Value()
    if obj is None:
        value.null_value = struct_pb2.NULL_VALUE
    elif isinstance(obj, bool):
        value.bool_value = obj
    elif isinstance(obj, int | float):
        value.number_value = float(obj)
    elif isinstance(obj, str):
        value.string_value = obj
    elif isinstance(obj, dict):
        struct = struct_pb2.Struct()
        for k, v in obj.items():
            struct.fields[str(k)].CopyFrom(_python_to_proto_value(v))
        value.struct_value.CopyFrom(struct)
    elif isinstance(obj, list):
        list_value = struct_pb2.ListValue()
        for item in obj:
            list_value.values.append(_python_to_proto_value(item))
        value.list_value.CopyFrom(list_value)
    else:
        # Fallback: try to convert to dict
        import json

        try:
            d = json.loads(json.dumps(obj, default=str))
            return _python_to_proto_value(d)
        except Exception:
            value.string_value = str(obj)
    return value
