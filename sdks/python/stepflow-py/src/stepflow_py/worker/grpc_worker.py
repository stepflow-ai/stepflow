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

Graceful shutdown:
  On SIGTERM/SIGINT, the worker stops accepting new tasks, waits for
  in-flight tasks to complete (with a configurable grace period), then
  reports any remaining in-flight tasks as failed via CompleteTask.
  The orchestrator's retry system will re-dispatch these tasks to another
  worker.

Task lifecycle:
1. Receive TaskAssignment from PullTasks stream
2. Call TaskHeartbeat (initial) — claims the task; if should_abort, skip execution
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
import random
import signal
import traceback
import uuid
from typing import TYPE_CHECKING, Any

import grpc
import grpc.aio
from google.protobuf import struct_pb2

from stepflow_py.proto import (
    CompleteTaskRequest,
    ComponentExecuteResponse,
    ComponentInfo,
    PullTasksRequest,
    TaskAssignment,
    TaskError,
    TaskHeartbeatRequest,
)
from stepflow_py.proto.common_pb2 import (
    TASK_ERROR_CODE_COMPONENT_FAILED,
    TASK_ERROR_CODE_INVALID_INPUT,
    TASK_ERROR_CODE_RESOURCE_UNAVAILABLE,
    TASK_ERROR_CODE_TIMEOUT,
    TASK_ERROR_CODE_WORKER_ERROR,
)
from stepflow_py.proto.orchestrator_pb2_grpc import OrchestratorServiceStub
from stepflow_py.proto.tasks_pb2_grpc import TasksServiceStub
from stepflow_py.worker.orchestrator_tracker import OrchestratorTracker

if TYPE_CHECKING:
    from stepflow_py.worker.server import ComponentEntry, StepflowServer

logger = logging.getLogger(__name__)

# Static worker configuration from environment variables.
# Read once at module import; does not change during worker lifetime.
_BLOB_URL: str = os.environ.get("STEPFLOW_BLOB_URL", "")
_BLOB_THRESHOLD_BYTES: int = int(os.environ.get("STEPFLOW_BLOB_THRESHOLD_BYTES", "0"))

# Queue name set at runtime by run_grpc_worker(); used as metric attribute.
_QUEUE_NAME: str = ""

# Tasks service URL set at runtime by run_grpc_worker(); used by
# OrchestratorTracker for GetOrchestratorForRun discovery.
_TASKS_URL: str = ""

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


# Grace period (seconds) to wait for in-flight tasks to finish after
# receiving a shutdown signal. Tasks still running after this period
# are reported as failed via CompleteTask.
_SHUTDOWN_GRACE_PERIOD: float = float(
    os.environ.get("STEPFLOW_SHUTDOWN_GRACE_SECS", "10")
)


class _InFlightTasks:
    """Thread-safe registry of in-flight tasks for graceful shutdown.

    Tracks task_id → (TaskAssignment, asyncio.Task) so the shutdown
    handler can enumerate in-flight work, wait for completion, and
    report failures for tasks that don't finish in time.
    """

    def __init__(self) -> None:
        self._tasks: dict[str, tuple[TaskAssignment, asyncio.Task[None]]] = {}
        self._lock = asyncio.Lock()

    async def register(
        self, task_id: str, assignment: TaskAssignment, task: asyncio.Task[None]
    ) -> None:
        async with self._lock:
            self._tasks[task_id] = (assignment, task)

    async def unregister(self, task_id: str) -> None:
        async with self._lock:
            self._tasks.pop(task_id, None)

    async def snapshot(self) -> list[tuple[str, TaskAssignment, asyncio.Task[None]]]:
        """Return a snapshot of all in-flight tasks."""
        async with self._lock:
            return [
                (tid, assignment, atask)
                for tid, (assignment, atask) in self._tasks.items()
            ]


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
    global _QUEUE_NAME, _TASKS_URL  # noqa: PLW0603
    _QUEUE_NAME = queue_name
    _TASKS_URL = tasks_url
    _ensure_metrics()

    worker_id = str(uuid.uuid4())
    logger.info(
        "Starting gRPC worker: worker_id=%s, tasks_url=%s, "
        "queue=%s, max_concurrent=%d, "
        "blob_url=%s, blob_threshold=%d",
        worker_id,
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

    # In-flight task registry for graceful shutdown
    in_flight = _InFlightTasks()

    # Shutdown event — set by signal handlers to trigger graceful shutdown
    shutdown_event = asyncio.Event()

    # Install signal handlers for graceful shutdown
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGTERM, signal.SIGINT):
        loop.add_signal_handler(sig, shutdown_event.set)

    consecutive_failures = 0
    had_successful_connection = False
    while not shutdown_event.is_set():
        try:
            await _pull_loop(
                server=server,
                tasks_url=tasks_url,
                queue_name=queue_name,
                components=components,
                semaphore=semaphore,
                max_concurrent=max_concurrent,
                worker_id=worker_id,
                in_flight=in_flight,
                shutdown_event=shutdown_event,
            )
            # Successful pull session — reset failure counter
            consecutive_failures = 0
            had_successful_connection = True
        except grpc.aio.AioRpcError as e:
            if shutdown_event.is_set():
                break
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
                break

            if consecutive_failures >= max_retries:
                logger.error(
                    "gRPC worker giving up after %d consecutive failures "
                    "(last: code=%s, %s)",
                    consecutive_failures,
                    e.code(),
                    e.details(),
                )
                break
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
            if shutdown_event.is_set():
                break
            consecutive_failures += 1
            if consecutive_failures >= max_retries:
                logger.error(
                    "gRPC worker giving up after %d consecutive failures",
                    consecutive_failures,
                )
                break
            logger.exception(
                "Unexpected error in pull loop. Reconnecting in 5s... (attempt %d/%d)",
                consecutive_failures,
                max_retries,
            )
            await asyncio.sleep(5)

    # --- Graceful shutdown ---
    await _graceful_shutdown(in_flight, worker_id)


async def _pull_loop(
    server: StepflowServer,
    tasks_url: str,
    queue_name: str,
    components: list[ComponentInfo],
    semaphore: asyncio.Semaphore,
    max_concurrent: int,
    worker_id: str,
    in_flight: _InFlightTasks,
    shutdown_event: asyncio.Event,
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
            worker_id=worker_id,
        )

        logger.info("Connecting to TasksService at %s...", tasks_url)
        stream = tasks_stub.PullTasks(request)

        async for task in stream:
            if shutdown_event.is_set():
                break
            if _tasks_pulled is not None:
                _tasks_pulled.add(1, {"queue_name": _QUEUE_NAME})
            await semaphore.acquire()
            handle = asyncio.create_task(
                _handle_task(server, task, semaphore, worker_id, in_flight),
            )
            await in_flight.register(task.task_id, task, handle)
    finally:
        if _connection_status is not None:
            _connection_status.add(-1, {"queue_name": _QUEUE_NAME})
        await channel.close()


async def _handle_task(
    server: StepflowServer,
    task: TaskAssignment,
    semaphore: asyncio.Semaphore,
    worker_id: str,
    in_flight: _InFlightTasks,
) -> None:
    """Handle a single task: claim, execute, complete."""
    orchestrator_url = (
        task.request.context.orchestrator_service_url
        if task.request.HasField("context")
        else ""
    )
    root_run_id = (
        task.request.context.root_run_id
        if task.request.HasField("context") and task.request.context.root_run_id
        else None
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

        # Create per-task orchestrator tracker for URL discovery
        tracker = OrchestratorTracker(
            url=orchestrator_url,
            run_id=run_id,
            root_run_id=root_run_id,
            tasks_url=_TASKS_URL,
        )

        # Open a channel to the run-owning orchestrator to claim the task
        if orchestrator_url:
            orch_channel = grpc.aio.insecure_channel(tracker.url)
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

            # Close claim channel; heartbeat loop manages its own
            await orch_channel.close()
            orch_channel = None

            # Start background heartbeat loop
            interval = task.heartbeat_interval_secs or 1
            heartbeat_task = asyncio.create_task(
                _heartbeat_loop(tracker, task.task_id, worker_id, interval, run_id)
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
                tracker,
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
                    server, component, input_data, req, path_params, tracker
                )
                # Convert output to proto Value
                proto_output = _python_to_proto_value(output)
                await _complete_task_success(
                    task,
                    proto_output,
                    tracker,
                )
            except Exception as e:
                logger.error("Component %s failed: %s", req.component, e, exc_info=True)
                error_code, error_data = _classify_exception(e)
                await _complete_task_error(
                    task,
                    str(e),
                    tracker,
                    code=error_code,
                    data=error_data,
                )

    except Exception:
        logger.exception("Failed to handle task %s", task.task_id)
        try:
            await _complete_task_error(
                task,
                traceback.format_exc(),
                tracker,
                code=TASK_ERROR_CODE_WORKER_ERROR,
            )
        except Exception:
            logger.exception("Failed to report error for task %s", task.task_id)
    finally:
        await in_flight.unregister(task.task_id)
        if heartbeat_task:
            heartbeat_task.cancel()
            try:
                await heartbeat_task
            except asyncio.CancelledError:
                pass
        if orch_channel:
            await orch_channel.close()
        semaphore.release()


async def _graceful_shutdown(
    in_flight: _InFlightTasks,
    worker_id: str,
) -> None:
    """Wait for in-flight tasks to complete, then report failures for stragglers.

    Called after the pull loop exits due to a shutdown signal. Waits up
    to ``_SHUTDOWN_GRACE_PERIOD`` seconds for in-flight tasks to finish
    naturally, then cancels and reports any remaining tasks as failed via
    CompleteTask so the orchestrator can retry them on another worker.
    """
    tasks = await in_flight.snapshot()
    if not tasks:
        logger.info("Graceful shutdown: no in-flight tasks")
        return

    logger.info(
        "Graceful shutdown: waiting up to %.0fs for %d in-flight task(s)",
        _SHUTDOWN_GRACE_PERIOD,
        len(tasks),
    )

    # Wait for in-flight tasks to complete within the grace period
    asyncio_tasks = [atask for _, _, atask in tasks]
    _done, pending = await asyncio.wait(asyncio_tasks, timeout=_SHUTDOWN_GRACE_PERIOD)

    if not pending:
        logger.info("Graceful shutdown: all tasks completed within grace period")
        return

    # Cancel remaining tasks and report them as failed
    logger.warning(
        "Graceful shutdown: %d task(s) still running after grace period, "
        "reporting as failed",
        len(pending),
    )

    remaining = await in_flight.snapshot()
    for task_id, assignment, atask in remaining:
        if not atask.done():
            atask.cancel()
            try:
                await atask
            except (asyncio.CancelledError, Exception):
                pass

        # Report failure to orchestrator so it can retry on another worker
        try:
            orch_url = (
                assignment.request.context.orchestrator_service_url
                if assignment.request.HasField("context")
                else ""
            )
            shutdown_tracker = OrchestratorTracker(
                url=orch_url,
                run_id=None,
                root_run_id=None,
                tasks_url=_TASKS_URL,
            )
            await _complete_task_error(
                assignment,
                f"task '{task_id}' failed: worker '{worker_id}' shutting down",
                shutdown_tracker,
                code=TASK_ERROR_CODE_TIMEOUT,
            )
            logger.info("Reported shutdown failure for task %s", task_id)
        except Exception:
            logger.exception("Failed to report shutdown failure for task %s", task_id)

    logger.info("Graceful shutdown complete")


async def _heartbeat_loop(
    tracker: OrchestratorTracker,
    task_id: str,
    worker_id: str,
    interval_secs: int,
    run_id: str | None = None,
) -> None:
    """Send periodic heartbeats until cancelled.

    Manages its own gRPC channel and recreates it when the tracker's URL
    changes (e.g., another operation discovered a new orchestrator).
    On gRPC UNAVAILABLE or NOT_FOUND, attempts discovery via the tracker.

    Each sleep is jittered by ±20% to avoid thundering-herd effects when
    many workers heartbeat against the same orchestrator.
    """
    jitter_min = interval_secs * 0.8
    jitter_max = interval_secs * 1.2
    current_url = tracker.url
    channel = grpc.aio.insecure_channel(current_url)
    stub = OrchestratorServiceStub(channel)
    try:
        while True:
            await asyncio.sleep(random.uniform(jitter_min, jitter_max))

            # If tracker URL changed (e.g., by CompleteTask discovery),
            # reconnect proactively
            if tracker.url != current_url:
                await channel.close()
                current_url = tracker.url
                channel = grpc.aio.insecure_channel(current_url)
                stub = OrchestratorServiceStub(channel)

            try:
                response = await stub.TaskHeartbeat(
                    TaskHeartbeatRequest(
                        task_id=task_id,
                        worker_id=worker_id,
                        run_id=run_id,
                    )
                )
                if _heartbeats_sent is not None:
                    _heartbeats_sent.add(1, {"queue_name": _QUEUE_NAME})
                if response.should_abort:
                    logger.info(
                        "Task %s abort requested (status=%s)",
                        task_id,
                        response.status,
                    )
                    break
            except grpc.aio.AioRpcError as e:
                if e.code() in (
                    grpc.StatusCode.UNAVAILABLE,
                    grpc.StatusCode.NOT_FOUND,
                ):
                    # Try to discover new orchestrator
                    if await tracker.discover():
                        await channel.close()
                        current_url = tracker.url
                        channel = grpc.aio.insecure_channel(current_url)
                        stub = OrchestratorServiceStub(channel)
                        continue
                # Discovery failed or non-recoverable error — stop
                logger.warning(
                    "Heartbeat failed for task %s: %s (code=%s)",
                    task_id,
                    e.details(),
                    e.code(),
                )
                break
    except asyncio.CancelledError:
        pass
    finally:
        await channel.close()


async def _execute_component(
    server: StepflowServer,
    component: ComponentEntry,
    input_data: Any,
    req: Any,
    path_params: dict[str, str],
    tracker: OrchestratorTracker | None = None,
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


def _extract_run_id(task: TaskAssignment) -> str | None:
    """Extract run_id from a task's observability context."""
    if task.request.HasField("observability"):
        obs = task.request.observability
        if obs.HasField("run_id"):
            return obs.run_id
    return None


async def _complete_task_with_retry(
    tracker: OrchestratorTracker,
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
    - gRPC NOT_FOUND: task not on this orchestrator, discover + retry

    Returns True if the result was delivered (or is no longer needed),
    False if all retries failed.
    """
    channel = grpc.aio.insecure_channel(tracker.url)
    try:
        stub = OrchestratorServiceStub(channel)
        attempt = 0
        discovered_for_not_found = False
        while True:
            try:
                await stub.CompleteTask(request)
                return True  # success

            except grpc.aio.AioRpcError as e:
                if e.code() == grpc.StatusCode.UNAVAILABLE:
                    attempt += 1

                    # On first UNAVAILABLE, try to discover the new
                    # orchestrator (failover or recovering).
                    if attempt == 1 and await tracker.discover():
                        attempt = 0  # reset for new URL
                        await channel.close()
                        channel = grpc.aio.insecure_channel(tracker.url)
                        stub = OrchestratorServiceStub(channel)
                        continue

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
                        channel = grpc.aio.insecure_channel(tracker.url)
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
                    # Task not on this orchestrator — discover once
                    if not discovered_for_not_found and await tracker.discover():
                        discovered_for_not_found = True
                        await channel.close()
                        channel = grpc.aio.insecure_channel(tracker.url)
                        stub = OrchestratorServiceStub(channel)
                        continue
                    # Same URL or already tried — terminal
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
    tracker: OrchestratorTracker,
) -> None:
    """Report successful task completion to the run-owning orchestrator."""
    if not tracker.url:
        logger.error(
            "No orchestrator URL for task %s — cannot report completion",
            task.task_id,
        )
        return

    run_id = _extract_run_id(task)
    request = CompleteTaskRequest(
        task_id=task.task_id,
        response=ComponentExecuteResponse(output=output),
        run_id=run_id,
    )
    if await _complete_task_with_retry(tracker, request, task.task_id):
        logger.debug("Completed task %s (success)", task.task_id)
        if _tasks_completed is not None:
            _tasks_completed.add(1, {"queue_name": _QUEUE_NAME, "outcome": "success"})


async def _complete_task_error(
    task: TaskAssignment,
    error_msg: str,
    tracker: OrchestratorTracker,
    *,
    code: int = TASK_ERROR_CODE_COMPONENT_FAILED,
    data: dict | None = None,
) -> None:
    """Report task failure to the run-owning orchestrator."""
    if not tracker.url:
        logger.error(
            "No orchestrator URL for task %s — cannot report error",
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
    run_id = _extract_run_id(task)
    request = CompleteTaskRequest(
        task_id=task.task_id,
        error=task_error,
        run_id=run_id,
    )
    if await _complete_task_with_retry(tracker, request, task.task_id):
        logger.debug("Completed task %s (error, code=%s)", task.task_id, code)
        if _tasks_completed is not None:
            _tasks_completed.add(1, {"queue_name": _QUEUE_NAME, "outcome": "error"})


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
