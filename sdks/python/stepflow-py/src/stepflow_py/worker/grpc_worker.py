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
import logging
import os
import signal
import uuid
from typing import TYPE_CHECKING, Any

import grpc
import grpc.aio

from stepflow_py.proto import (
    CompleteTaskRequest,
    ComponentInfo,
    ListComponentsResult,
    PullTasksRequest,
)
from stepflow_py.proto.tasks_pb2_grpc import TasksServiceStub

# Re-export shared task handling functions for backward compatibility.
# grpc_context.py imports _proto_value_to_python and _python_to_proto_value
# from this module.
from stepflow_py.worker.task_handler import (
    _ensure_metrics,
    handle_task,
)
from stepflow_py.worker.task_handler import (
    build_component_info_list as _build_component_info_list,
)

if TYPE_CHECKING:
    from stepflow_py.worker.server import StepflowServer

logger = logging.getLogger(__name__)


# Tasks service URL set at runtime by run_grpc_worker(); used by
# OrchestratorTracker for GetOrchestratorForRun discovery.
_TASKS_URL: str = ""


# --- OpenTelemetry metrics instruments (optional dependency) ---
# Connection-status counter is gRPC-specific (not shared with NATS worker).
_OTEL_METRICS_AVAILABLE = False
_tasks_pulled: Any = None
_connection_status: Any = None
_grpc_metrics_initialized = False

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

    Tracks task_id -> (TaskAssignment, asyncio.Task) so the shutdown
    handler can enumerate in-flight work, wait for completion, and
    report failures for tasks that don't finish in time.
    """

    def __init__(self) -> None:
        self._tasks: dict[str, tuple[Any, asyncio.Task[None]]] = {}
        self._lock = asyncio.Lock()

    async def register(
        self, task_id: str, assignment: Any, task: asyncio.Task[None]
    ) -> None:
        async with self._lock:
            self._tasks[task_id] = (assignment, task)

    async def unregister(self, task_id: str) -> None:
        async with self._lock:
            self._tasks.pop(task_id, None)

    async def snapshot(self) -> list[tuple[str, Any, asyncio.Task[None]]]:
        """Return a snapshot of all in-flight tasks."""
        async with self._lock:
            return [
                (tid, assignment, atask)
                for tid, (assignment, atask) in self._tasks.items()
            ]


def _ensure_grpc_metrics() -> None:
    """Lazily create gRPC-specific OTel metric instruments."""
    global _tasks_pulled, _connection_status, _grpc_metrics_initialized  # noqa: PLW0603
    if _grpc_metrics_initialized or not _OTEL_METRICS_AVAILABLE:
        return
    _grpc_metrics_initialized = True
    try:
        meter = otel_metrics.get_meter("stepflow-grpc-worker")
        _tasks_pulled = meter.create_counter(
            "worker.tasks_pulled_total",
            description="Total tasks pulled from the queue",
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
    global _TASKS_URL  # noqa: PLW0603
    _TASKS_URL = tasks_url
    _ensure_metrics("stepflow-grpc-worker", queue_name)
    _ensure_grpc_metrics()

    worker_id = str(uuid.uuid4())
    logger.info(
        "Starting gRPC worker: worker_id=%s, tasks_url=%s, queue=%s, max_concurrent=%d",
        worker_id,
        tasks_url,
        queue_name,
        max_concurrent,
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
    worker_id: str,
    in_flight: _InFlightTasks,
    shutdown_event: asyncio.Event,
) -> None:
    """Single pull session — connects, pulls tasks, processes them."""
    channel = grpc.aio.insecure_channel(tasks_url)
    if _connection_status is not None:
        _connection_status.add(1, {"queue_name": queue_name})
    try:
        tasks_stub = TasksServiceStub(channel)

        request = PullTasksRequest(
            queue_name=queue_name,
            worker_id=worker_id,
        )

        logger.info("Connecting to TasksService at %s...", tasks_url)
        stream = tasks_stub.PullTasks(request)

        async for task in stream:
            if shutdown_event.is_set():
                break

            # Handle discovery tasks (ListComponentsRequest) inline
            if task.HasField("list_components"):
                asyncio.create_task(_handle_list_components_grpc(task, components))
                continue

            if _tasks_pulled is not None:
                _tasks_pulled.add(1, {"queue_name": queue_name})
            await semaphore.acquire()
            handle = asyncio.create_task(
                handle_task(
                    server,
                    task,
                    semaphore,
                    worker_id,
                    queue_name,
                    tracer_name="stepflow-grpc-worker",
                ),
            )
            await in_flight.register(task.task_id, task, handle)
    finally:
        if _connection_status is not None:
            _connection_status.add(-1, {"queue_name": queue_name})
        await channel.close()


async def _handle_list_components_grpc(
    task: Any,
    components: list[ComponentInfo],
) -> None:
    """Handle a ListComponentsRequest task from the orchestrator.

    Responds via CompleteTask with a ListComponentsResult containing
    the worker's available components.
    """
    from stepflow_py.worker.task_handler import complete_task_with_retry

    orchestrator_url = (
        task.context.orchestrator_service_url if task.HasField("context") else ""
    )
    if not orchestrator_url:
        logger.error(
            "No orchestrator_service_url for list_components task %s",
            task.task_id,
        )
        return

    logger.info(
        "Handling list_components discovery task %s (%d components)",
        task.task_id,
        len(components),
    )

    result = ListComponentsResult(components=components)
    request = CompleteTaskRequest(
        task_id=task.task_id,
        list_components=result,
    )

    if await complete_task_with_retry(orchestrator_url, request, task.task_id):
        logger.debug(
            "Completed list_components task %s (%d components)",
            task.task_id,
            len(components),
        )


async def _graceful_shutdown(
    in_flight: _InFlightTasks,
    worker_id: str,
) -> None:
    """Wait for in-flight tasks to complete, then cancel stragglers.

    Called after the pull loop exits due to a shutdown signal. Waits up
    to ``_SHUTDOWN_GRACE_PERIOD`` seconds for in-flight tasks to finish
    naturally, then cancels any remaining tasks. The orchestrator's
    heartbeat-based crash detection will re-dispatch them to another worker.
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

    # Cancel remaining tasks — the orchestrator's heartbeat timeout will
    # detect the missing worker and re-dispatch.
    logger.warning(
        "Graceful shutdown: %d task(s) still running after grace period, cancelling",
        len(pending),
    )

    for atask in pending:
        atask.cancel()
        try:
            await atask
        except (asyncio.CancelledError, Exception):
            pass

    logger.info("Graceful shutdown complete")
