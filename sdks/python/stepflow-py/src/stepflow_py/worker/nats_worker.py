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

"""NATS JetStream worker for Stepflow.

Connects to a NATS JetStream server, pulls task assignments from a
durable consumer, and delegates execution to the shared task handler.
Task completion is reported via gRPC OrchestratorService using the
orchestrator_service_url from each task's TaskContext.

Special ``/__stepflow/list_components`` tasks are handled inline: the
worker responds with a ``ListComponentsResult`` via ``CompleteTask``
so the orchestrator can discover available components.

Worker configuration:
  - STEPFLOW_NATS_URL: NATS server URL (default: nats://localhost:4222)
  - STEPFLOW_NATS_CONSUMER: Durable consumer name (default: stepflow-default)
  - STEPFLOW_BLOB_URL: URL for the blob gRPC/HTTP API
  - STEPFLOW_BLOB_THRESHOLD_BYTES: Byte threshold for auto-blobification
"""

from __future__ import annotations

import asyncio
import json
import logging
import uuid
from typing import TYPE_CHECKING, Any

import nats
from nats.js.api import ConsumerConfig, DeliverPolicy

from stepflow_py.proto import (
    CompleteTaskRequest,
    ComponentInfo,
    ListComponentsResult,
    TaskAssignment,
)
from stepflow_py.worker.task_handler import (
    _ensure_metrics,
    complete_task_with_retry,
    handle_task,
)

if TYPE_CHECKING:
    from stepflow_py.worker.server import StepflowServer

logger = logging.getLogger(__name__)

# --- OpenTelemetry metrics instruments (optional dependency) ---
_OTEL_METRICS_AVAILABLE = False
_tasks_received: Any = None
_nats_metrics_initialized = False

try:
    from opentelemetry import metrics as otel_metrics

    _OTEL_METRICS_AVAILABLE = True
except ImportError:
    pass


def _ensure_nats_metrics() -> None:
    """Lazily create NATS-specific OTel metric instruments."""
    global _tasks_received, _nats_metrics_initialized  # noqa: PLW0603
    if _nats_metrics_initialized or not _OTEL_METRICS_AVAILABLE:
        return
    _nats_metrics_initialized = True
    try:
        meter = otel_metrics.get_meter("stepflow-nats-worker")
        _tasks_received = meter.create_counter(
            "worker.nats_tasks_received_total",
            description="Total tasks received from NATS JetStream",
        )
    except Exception:
        pass


class _StreamNotFoundError(Exception):
    """Raised when the NATS JetStream stream does not exist yet.

    This is not a connection failure — the orchestrator simply hasn't
    created the stream. The outer loop retries without counting it as
    a failure.
    """

    def __init__(self, stream: str) -> None:
        super().__init__(f"Stream {stream} not found")
        self.stream = stream


# Discovery component path used by the orchestrator for component listing.
_LIST_COMPONENTS_PATH = "/__stepflow/list_components"


async def run_nats_worker(
    server: StepflowServer,
    nats_url: str,
    stream: str,
    consumer: str,
    max_concurrent: int = 4,
    max_retries: int = 15,
) -> None:
    """Run the NATS JetStream worker.

    Connects to NATS, subscribes to a JetStream stream via a durable pull
    consumer, and processes task assignments. Component execution and
    completion reporting are delegated to the shared task handler.

    Each stream uses WorkQueue retention (one worker pool per stream).
    Tasks are published to a fixed ``{stream}.tasks`` subject within the
    stream.

    Args:
        server: StepflowServer with registered components.
        nats_url: NATS server URL (e.g., "nats://localhost:4222").
        stream: JetStream stream name (e.g., "PYTHON_TASKS").
        consumer: Durable consumer name (e.g., "python-workers").
        max_concurrent: Maximum concurrent task executions.
        max_retries: Maximum consecutive connection failures before giving up.
    """
    _ensure_metrics("stepflow-nats-worker", consumer)
    _ensure_nats_metrics()

    worker_id = str(uuid.uuid4())

    # Build component info list from registered components
    components = _build_component_info_list(server)
    if not components:
        logger.error("No components registered — nothing to serve")
        return

    logger.info(
        "Starting NATS worker: worker_id=%s, nats_url=%s, "
        "stream=%s, consumer=%s, max_concurrent=%d, components=%s",
        worker_id,
        nats_url,
        stream,
        consumer,
        max_concurrent,
        ", ".join(c.name for c in components),
    )

    semaphore = asyncio.Semaphore(max_concurrent)

    consecutive_failures = 0
    had_successful_connection = False
    while True:
        try:
            await _fetch_loop(
                server=server,
                nats_url=nats_url,
                stream=stream,
                consumer=consumer,
                components=components,
                semaphore=semaphore,
                max_concurrent=max_concurrent,
                worker_id=worker_id,
            )
            # Successful session — reset failure counter
            consecutive_failures = 0
            had_successful_connection = True
        except _StreamNotFoundError:
            # Stream not yet created by the orchestrator — retry without
            # counting as a failure (the sleep already happened inside).
            logger.debug("Stream not found, retrying...")
            continue
        except nats.errors.ConnectionClosedError:
            if had_successful_connection:
                logger.info(
                    "NATS connection closed after successful session. Shutting down."
                )
                return
            consecutive_failures += 1
            if consecutive_failures >= max_retries:
                logger.error(
                    "NATS worker giving up after %d consecutive failures",
                    consecutive_failures,
                )
                return
            logger.warning(
                "NATS connection closed. Reconnecting in 2s... (attempt %d/%d)",
                consecutive_failures,
                max_retries,
            )
            await asyncio.sleep(2)
        except Exception:
            consecutive_failures += 1
            if consecutive_failures >= max_retries:
                logger.error(
                    "NATS worker giving up after %d consecutive failures",
                    consecutive_failures,
                )
                return
            logger.exception(
                "Unexpected error in NATS fetch loop. Reconnecting in 5s... "
                "(attempt %d/%d)",
                consecutive_failures,
                max_retries,
            )
            await asyncio.sleep(5)


async def _fetch_loop(
    server: StepflowServer,
    nats_url: str,
    stream: str,
    consumer: str,
    components: list[ComponentInfo],
    semaphore: asyncio.Semaphore,
    max_concurrent: int,
    worker_id: str,
) -> None:
    """Single NATS session — connect, subscribe, fetch and process tasks."""
    nc = await nats.connect(nats_url)
    try:
        js = nc.jetstream()

        # Subscribe to the fixed internal subject within this stream.
        # The orchestrator publishes to "{stream}.tasks".
        subject = f"{stream}.tasks"

        logger.info(
            "Subscribing to NATS stream %s (subject %s) with consumer %s...",
            stream,
            subject,
            consumer,
        )

        try:
            sub = await js.pull_subscribe(
                subject,
                durable=consumer,
                config=ConsumerConfig(
                    deliver_policy=DeliverPolicy.ALL,
                ),
            )
        except nats.js.errors.NotFoundError:
            logger.info("Stream %s not yet created, waiting...", stream)
            await asyncio.sleep(2)
            raise _StreamNotFoundError(stream) from None

        logger.info("NATS worker connected and listening for tasks")

        while True:
            # Acquire a concurrency slot BEFORE pulling from NATS.
            # This ensures we only pull tasks we can immediately process,
            # providing natural backpressure to the queue.
            await semaphore.acquire()

            try:
                msgs = await sub.fetch(batch=1, timeout=5)
            except nats.errors.TimeoutError:
                # No messages available — release slot and retry
                semaphore.release()
                continue

            msg = msgs[0]
            if _tasks_received is not None:
                _tasks_received.add(1, {"consumer": consumer})

            # Ack the NATS message immediately upon receipt, before starting
            # the task. NATS is purely a delivery mechanism — the orchestrator
            # is the single source of truth for task lifecycle (heartbeat
            # timeouts, retry budgets, attempt counting).
            #
            # If the worker crashes between ack and the first heartbeat, the
            # orchestrator's queue timeout detects the missing heartbeat and
            # retries the task through its normal retry path. Delaying ack
            # (e.g., until after heartbeat claim) would risk double-dispatch:
            # NATS redelivers the message while the orchestrator also retries,
            # producing two concurrent attempts that must be reconciled.
            # Ack-on-receive avoids this by ensuring each message is processed
            # by exactly one codepath.
            await msg.ack()

            # Deserialize the TaskAssignment from protobuf bytes
            task = TaskAssignment()
            task.ParseFromString(msg.data)

            logger.debug(
                "Received task %s: component=%s",
                task.task_id,
                task.request.component,
            )

            # Handle discovery tasks inline (release semaphore — these
            # are lightweight and don't count toward concurrency).
            if task.request.component == _LIST_COMPONENTS_PATH:
                semaphore.release()
                asyncio.create_task(_handle_list_components(task, components, consumer))
                continue

            # Normal task — semaphore already acquired, delegate to handler.
            # handle_task claims via heartbeat, executes, and reports result.
            # It releases the semaphore when done.
            asyncio.create_task(
                handle_task(
                    server,
                    task,
                    semaphore,
                    worker_id,
                    consumer,
                    tracer_name="stepflow-nats-worker",
                ),
            )
    finally:
        await nc.close()


async def _handle_list_components(
    task: TaskAssignment,
    components: list[ComponentInfo],
    consumer: str,
) -> None:
    """Handle a /__stepflow/list_components discovery task.

    Responds via CompleteTask with a ListComponentsResult containing
    the worker's available components.
    """
    orchestrator_url = (
        task.request.context.orchestrator_service_url
        if task.request.HasField("context")
        else ""
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


def _build_component_info_list(
    server: StepflowServer,
) -> list[ComponentInfo]:
    """Build proto ComponentInfo list from registered components."""
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
