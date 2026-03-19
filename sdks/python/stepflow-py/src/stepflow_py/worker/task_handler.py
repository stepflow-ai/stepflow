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

"""Shared task handling logic for Stepflow workers.

Contains the core task execution lifecycle shared by both gRPC pull-based
and NATS JetStream workers:
- Heartbeat claim and background heartbeat loop
- Component execution
- Task completion (success, error, retry)
- Exception classification
- Proto value conversion helpers

Both transport implementations receive a ``TaskAssignment`` proto and
report completion via gRPC ``OrchestratorService`` using the
``orchestrator_service_url`` from each task's ``TaskContext``.
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

if TYPE_CHECKING:
    from stepflow_py.worker.server import ComponentEntry, StepflowServer

logger = logging.getLogger(__name__)

# Static worker configuration from environment variables.
# Read once at module import; does not change during worker lifetime.
_BLOB_URL: str = os.environ.get("STEPFLOW_BLOB_URL", "")
_BLOB_THRESHOLD_BYTES: int = int(os.environ.get("STEPFLOW_BLOB_THRESHOLD_BYTES", "0"))

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
# Instruments are lazily created on first use because this module is
# imported before setup_observability() installs a real MeterProvider.
_OTEL_METRICS_AVAILABLE = False
_tasks_completed: Any = None
_heartbeats_sent: Any = None
_metrics_initialized = False

try:
    from opentelemetry import metrics as otel_metrics

    _OTEL_METRICS_AVAILABLE = True
except ImportError:
    pass


def _ensure_metrics(meter_name: str, queue_name: str) -> None:
    """Lazily create OTel metric instruments after MeterProvider is set.

    Args:
        meter_name: Name for the OTel meter (e.g., "stepflow-grpc-worker").
        queue_name: Queue name used as metric attribute (stored for later use).
    """
    global _tasks_completed, _heartbeats_sent, _metrics_initialized  # noqa: PLW0603
    if _metrics_initialized or not _OTEL_METRICS_AVAILABLE:
        return
    _metrics_initialized = True
    try:
        meter = otel_metrics.get_meter(meter_name)
        _tasks_completed = meter.create_counter(
            "worker.tasks_completed_total",
            description="Total tasks completed (success or error)",
        )
        _heartbeats_sent = meter.create_counter(
            "worker.heartbeats_sent_total",
            description="Total heartbeats sent to the orchestrator",
        )
    except Exception:
        pass


async def handle_task(
    server: StepflowServer,
    task: TaskAssignment,
    semaphore: asyncio.Semaphore,
    worker_id: str,
    queue_name: str,
    tracer_name: str = "stepflow-worker",
) -> None:
    """Handle a single task: claim, execute, complete.

    This is the core task execution lifecycle shared by all transport
    implementations. It:
    1. Opens a gRPC channel to the run-owning orchestrator
    2. Sends an initial heartbeat to claim the task
    3. Starts a background heartbeat loop
    4. Executes the component
    5. Reports completion (success or error) via CompleteTask
    6. Cleans up the heartbeat loop and channel

    Args:
        server: StepflowServer with registered components.
        task: The TaskAssignment proto to process.
        semaphore: Concurrency-limiting semaphore (released in finally).
        worker_id: Unique worker instance ID.
        queue_name: Queue name for metric attributes.
        tracer_name: Name for the OTel tracer.
    """
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
        run_id = obs.run_id if obs.HasField("run_id") else None
        _set_execution_context(
            run_id=run_id,
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
                tracer = otel_trace.get_tracer(tracer_name)
                _span_cm = tracer.start_as_current_span(
                    f"execute {req.component}",
                    context=parent_ctx,
                    attributes={
                        "stepflow.task_id": task.task_id,
                        "stepflow.component": req.component,
                        "stepflow.attempt": req.attempt,
                        "stepflow.queue_name": queue_name,
                    },
                )
            except Exception:
                logger.debug("Failed to create trace span", exc_info=True)

        # Use the span context manager if available, otherwise a no-op
        from contextlib import nullcontext

        span_ctx = _span_cm if _span_cm is not None else nullcontext()

        # Open a shared channel to the run-owning orchestrator for
        # heartbeats and CompleteTask
        if orchestrator_url:
            orch_channel = grpc.aio.insecure_channel(orchestrator_url)
            stub = OrchestratorServiceStub(orch_channel)

            # Send initial heartbeat to claim the task. Retries on UNAVAILABLE
            # (run is being recovered — task_id may be re-registered shortly).
            claimed = False
            for claim_attempt in range(5):
                try:
                    response = await stub.TaskHeartbeat(
                        TaskHeartbeatRequest(
                            task_id=task.task_id,
                            worker_id=worker_id,
                            run_id=run_id,
                        )
                    )
                    if response.should_abort:
                        logger.warning(
                            "Task %s cannot be claimed (status=%s), skipping execution",
                            task.task_id,
                            response.status,
                        )
                        return
                    claimed = True
                    break
                except grpc.aio.AioRpcError as e:
                    if e.code() == grpc.StatusCode.UNAVAILABLE:
                        # Run is recovering — retry
                        logger.info(
                            "Task %s claim: UNAVAILABLE (recovering?), "
                            "retrying in 1s (%d/5)",
                            task.task_id,
                            claim_attempt + 1,
                        )
                        await asyncio.sleep(1)
                        continue
                    logger.error(
                        "Initial heartbeat failed for task %s: %s (code=%s)",
                        task.task_id,
                        e.details(),
                        e.code(),
                    )
                    return
            if not claimed:
                logger.warning(
                    "Task %s not ready after 5 attempts, skipping", task.task_id
                )
                return

            # Start background heartbeat loop
            interval = task.heartbeat_interval_secs or 1
            heartbeat_task = asyncio.create_task(
                _heartbeat_loop(
                    stub, task.task_id, worker_id, interval, run_id, queue_name
                )
            )

        # Convert proto Value to Python dict
        input_data = proto_value_to_python(req.input)

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
                queue_name=queue_name,
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
                output = await execute_component(
                    server, component, input_data, req, path_params
                )
                # Convert output to proto Value
                proto_output = python_to_proto_value(output)
                await _complete_task_success(
                    task,
                    proto_output,
                    queue_name=queue_name,
                )
            except Exception as e:
                logger.error("Component %s failed: %s", req.component, e, exc_info=True)
                error_code, error_data = classify_exception(e)
                await _complete_task_error(
                    task,
                    str(e),
                    code=error_code,
                    data=error_data,
                    queue_name=queue_name,
                )

    except Exception:
        logger.exception("Failed to handle task %s", task.task_id)
        try:
            await _complete_task_error(
                task,
                traceback.format_exc(),
                code=TASK_ERROR_CODE_WORKER_ERROR,
                queue_name=queue_name,
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
    worker_id: str,
    interval_secs: int,
    run_id: str | None = None,
    queue_name: str = "",
) -> None:
    """Send periodic heartbeats until cancelled."""
    try:
        while True:
            await asyncio.sleep(interval_secs)
            try:
                response = await stub.TaskHeartbeat(
                    TaskHeartbeatRequest(
                        task_id=task_id,
                        worker_id=worker_id,
                        run_id=run_id,
                    )
                )
                if _heartbeats_sent is not None:
                    _heartbeats_sent.add(1, {"queue_name": queue_name})
                if response.should_abort:
                    logger.info(
                        "Task %s abort requested (status=%s)",
                        task_id,
                        response.status,
                    )
                    break
            except grpc.aio.AioRpcError as e:
                if e.code() == grpc.StatusCode.UNAVAILABLE:
                    # Run is recovering — keep heartbeating
                    logger.info(
                        "Task %s heartbeat: UNAVAILABLE (recovering?), continuing",
                        task_id,
                    )
                    continue
                logger.warning(
                    "Heartbeat failed for task %s: %s (code=%s)",
                    task_id,
                    e.details(),
                    e.code(),
                )
                break
    except asyncio.CancelledError:
        pass


async def execute_component(
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
        from stepflow_py.worker.orchestrator_tracker import OrchestratorTracker

        orch_url = (
            req.context.orchestrator_service_url if req.HasField("context") else ""
        )
        root_run_id = (
            req.context.root_run_id
            if req.HasField("context") and req.context.root_run_id
            else None
        )
        obs_run_id = (
            req.observability.run_id
            if req.HasField("observability") and req.observability.HasField("run_id")
            else None
        )
        tracker = OrchestratorTracker(
            url=orch_url,
            run_id=obs_run_id,
            root_run_id=root_run_id,
            tasks_url="",
        )
        context = GrpcContext(
            orchestrator_tracker=tracker,
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


def classify_exception(exc: Exception) -> tuple[int, dict | None]:
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


def _extract_run_id(task: TaskAssignment) -> str | None:
    """Extract run_id from a task's observability context."""
    if task.request.HasField("observability"):
        obs = task.request.observability
        if obs.HasField("run_id"):
            return obs.run_id
    return None


async def complete_task_with_retry(
    orchestrator_url: str,
    request: CompleteTaskRequest,
    task_id: str,
    *,
    max_retries: int = 5,
    base_delay: float = 2.0,
) -> bool:
    """Send CompleteTask with retry on transient failures.

    Uses gRPC error codes for all failure signaling:
    - Success: non-error response (result delivered)
    - gRPC UNAVAILABLE: orchestrator down or recovering, retry with backoff
    - gRPC NOT_FOUND: task not on this orchestrator, terminal (not an error)

    Returns True if the result was delivered (or is no longer needed),
    False if all retries failed.
    """
    channel = grpc.aio.insecure_channel(orchestrator_url)
    try:
        stub = OrchestratorServiceStub(channel)
        attempt = 0
        while True:
            try:
                await stub.CompleteTask(request)
                return True  # success

            except grpc.aio.AioRpcError as e:
                if e.code() == grpc.StatusCode.UNAVAILABLE:
                    attempt += 1
                    if attempt <= max_retries:
                        delay = min(base_delay * (2**attempt), 30.0)
                        logger.warning(
                            "CompleteTask UNAVAILABLE for %s, retry in %.1fs (%d/%d)",
                            task_id,
                            delay,
                            attempt,
                            max_retries,
                        )
                        await asyncio.sleep(delay)
                        await channel.close()
                        channel = grpc.aio.insecure_channel(orchestrator_url)
                        stub = OrchestratorServiceStub(channel)
                    else:
                        logger.error(
                            "CompleteTask failed for %s after %d retries: %s",
                            task_id,
                            max_retries,
                            e.code(),
                        )
                        return False

                elif e.code() == grpc.StatusCode.NOT_FOUND:
                    # Task already completed or cleaned up — not an error
                    logger.info(
                        "CompleteTask NOT_FOUND for %s — task not on "
                        "orchestrator, skipping",
                        task_id,
                    )
                    return True

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


async def _complete_task_success(
    task: TaskAssignment,
    output: struct_pb2.Value,
    *,
    queue_name: str = "",
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

    run_id = _extract_run_id(task)
    request = CompleteTaskRequest(
        task_id=task.task_id,
        response=ComponentExecuteResponse(output=output),
        run_id=run_id,
    )
    if await complete_task_with_retry(orchestrator_url, request, task.task_id):
        logger.debug("Completed task %s (success)", task.task_id)
        if _tasks_completed is not None:
            _tasks_completed.add(1, {"queue_name": queue_name, "outcome": "success"})


async def _complete_task_error(
    task: TaskAssignment,
    error_msg: str,
    *,
    code: int = TASK_ERROR_CODE_COMPONENT_FAILED,
    data: dict | None = None,
    queue_name: str = "",
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
            proto_data.fields[str(k)].CopyFrom(python_to_proto_value(v))
        task_error.data.CopyFrom(proto_data)
    run_id = _extract_run_id(task)
    request = CompleteTaskRequest(
        task_id=task.task_id,
        error=task_error,
        run_id=run_id,
    )
    if await complete_task_with_retry(orchestrator_url, request, task.task_id):
        logger.debug("Completed task %s (error, code=%s)", task.task_id, code)
        if _tasks_completed is not None:
            _tasks_completed.add(1, {"queue_name": queue_name, "outcome": "error"})


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


def proto_value_to_python(value: struct_pb2.Value) -> Any:
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
            k: proto_value_to_python(v) for k, v in value.struct_value.fields.items()
        }
    elif kind == "list_value":
        return [proto_value_to_python(v) for v in value.list_value.values]
    else:
        return None


def python_to_proto_value(obj: Any) -> struct_pb2.Value:
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
            struct.fields[str(k)].CopyFrom(python_to_proto_value(v))
        value.struct_value.CopyFrom(struct)
    elif isinstance(obj, list):
        list_value = struct_pb2.ListValue()
        for item in obj:
            list_value.values.append(python_to_proto_value(item))
        value.list_value.CopyFrom(list_value)
    else:
        # Fallback: try to convert to dict
        import json

        try:
            d = json.loads(json.dumps(obj, default=str))
            return python_to_proto_value(d)
        except Exception:
            value.string_value = str(obj)
    return value
