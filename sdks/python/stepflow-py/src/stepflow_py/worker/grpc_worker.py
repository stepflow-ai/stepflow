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
from stepflow_py.proto.orchestrator_pb2_grpc import OrchestratorServiceStub
from stepflow_py.proto.tasks_pb2_grpc import TasksServiceStub

if TYPE_CHECKING:
    from stepflow_py.worker.server import ComponentEntry, StepflowServer

logger = logging.getLogger(__name__)

# Static worker configuration from environment variables.
# Read once at module import; does not change during worker lifetime.
_BLOB_URL: str = os.environ.get("STEPFLOW_BLOB_URL", "")
_BLOB_THRESHOLD_BYTES: int = int(os.environ.get("STEPFLOW_BLOB_THRESHOLD_BYTES", "0"))


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
        except grpc.aio.AioRpcError as e:
            consecutive_failures += 1
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
            await semaphore.acquire()
            asyncio.create_task(
                _handle_task(server, task, semaphore),
            )
    finally:
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
            await _complete_task_error(
                task,
                str(e),
                channel=orch_channel,
            )

    except Exception:
        logger.exception("Failed to handle task %s", task.task_id)
        try:
            await _complete_task_error(
                task,
                traceback.format_exc(),
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
    stub: OrchestratorServiceStub,
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

    # Reuse shared channel if available, otherwise create a new one
    own_channel = False
    if channel is None:
        channel = grpc.aio.insecure_channel(orchestrator_url)
        own_channel = True

    try:
        stub = OrchestratorServiceStub(channel)
        request = CompleteTaskRequest(
            task_id=task.task_id,
            response=ComponentExecuteResponse(output=output),
        )
        await stub.CompleteTask(request)
        logger.debug("Completed task %s (success)", task.task_id)
    except grpc.aio.AioRpcError as e:
        logger.error(
            "Failed to report completion for task %s: %s (code=%s)",
            task.task_id,
            e.details(),
            e.code(),
        )
    finally:
        if own_channel:
            await channel.close()


async def _complete_task_error(
    task: TaskAssignment,
    error_msg: str,
    *,
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

    # Reuse shared channel if available, otherwise create a new one
    own_channel = False
    if channel is None:
        channel = grpc.aio.insecure_channel(orchestrator_url)
        own_channel = True

    try:
        stub = OrchestratorServiceStub(channel)
        request = CompleteTaskRequest(
            task_id=task.task_id,
            error=TaskError(
                code=4,  # TASK_ERROR_CODE_COMPONENT_FAILED
                message=error_msg,
            ),
        )
        await stub.CompleteTask(request)
        logger.debug("Completed task %s (error)", task.task_id)
    except grpc.aio.AioRpcError as e:
        logger.error(
            "Failed to report error for task %s: %s (code=%s)",
            task.task_id,
            e.details(),
            e.code(),
        )
    finally:
        if own_channel:
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
    """Convert a protobuf Value to a Python object."""
    from google.protobuf.json_format import MessageToDict

    # MessageToDict converts protobuf Value to Python dict/list/scalar
    return MessageToDict(value, preserving_proto_field_name=True)


def _python_to_proto_value(obj: Any) -> struct_pb2.Value:
    """Convert a Python object to a protobuf Value."""
    from google.protobuf.json_format import ParseDict

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
        ParseDict(obj, struct)
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
