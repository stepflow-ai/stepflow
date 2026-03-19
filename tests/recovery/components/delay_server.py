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

"""Delay component for recovery integration tests (gRPC pull transport).

Each invocation blocks until explicitly released via the delay-control HTTP API,
writes a tracker record to a JSONL file, and returns the input payload along
with execution metadata.

Delays never auto-complete — they block indefinitely until released. This makes
tests fully deterministic: every step completes only when the test explicitly
releases it via the API.

Transport
---------
The worker connects to both orchestrators via gRPC pull transport
(``run_grpc_worker``). Each connection runs in a persistent reconnect loop
so the worker automatically reconnects when an orchestrator crashes and
restarts.

Delay-control API (HTTP on port 8080)
-------------------------------------
- ``GET  /health`` — health check for Docker
- ``GET  /delay?run_id=...&step_id=...`` — check whether a delay is pending
- ``POST /delay?run_id=...&step_id=...`` — release a pending delay
- ``POST /delay/release-all`` — release all pending delays at once

Both query parameters are optional.  When *run_id* is omitted the lookup
matches any run (useful for subflows whose run ID is not known to the test).
"""

import asyncio
import json
import logging
import os
import signal
import time
from datetime import datetime, timezone

import msgspec
import uvicorn
from fastapi import FastAPI, Query
from fastapi.responses import JSONResponse

from stepflow_py.worker import StepflowContext, StepflowServer
from stepflow_py.worker.grpc_worker import run_grpc_worker

TRACKER_FILE = os.environ.get("TRACKER_FILE", "/tracker/executions.jsonl")

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# SIGUSR1 handler for test-triggered crashes. Calling os._exit() causes the
# container's PID 1 to exit, which Docker's restart policy will auto-restart.
signal.signal(signal.SIGUSR1, lambda *_: os._exit(1))

server = StepflowServer()

# ---------------------------------------------------------------------------
# Delay registry — tracks in-flight delays so tests can query and release them
# ---------------------------------------------------------------------------

# Maps (run_id, step_label) -> {"event": asyncio.Event, "started_at": float}
_pending_delays: dict[tuple[str, str], dict] = {}


def _find_delay(
    run_id: str | None,
    step_id: str | None,
) -> tuple[tuple[str, str], dict] | None:
    """Find a pending delay matching the given filters.

    Returns ``(key, entry)`` or ``None``.
    """
    for key, entry in _pending_delays.items():
        k_run_id, k_step_label = key
        if run_id is not None and k_run_id != run_id:
            continue
        if step_id is not None and k_step_label != step_id:
            continue
        return key, entry
    return None


# ---------------------------------------------------------------------------
# Delay component
# ---------------------------------------------------------------------------


class DelayInput(msgspec.Struct):
    seconds: float = 5.0
    payload: dict | str | int | float | bool | None = None
    step_label: str = ""
    should_fail: bool = False


class DelayOutput(msgspec.Struct):
    payload: dict | str | int | float | bool | None
    step_label: str
    executed_at: str
    duration: float
    attempt: int
    tracker_attempt: int


def _count_tracker_records(run_id: str, step_label: str) -> int:
    """Count existing tracker records for this run_id + step_label."""
    count = 0
    try:
        with open(TRACKER_FILE) as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    record = json.loads(line)
                    if (
                        record.get("run_id") == run_id
                        and record.get("step_label") == step_label
                    ):
                        count += 1
                except json.JSONDecodeError:
                    continue
    except FileNotFoundError:
        pass
    return count


@server.component
async def delay(input: DelayInput, context: StepflowContext) -> DelayOutput:
    start = time.monotonic()
    run_id = context.run_id or ""
    key = (run_id, input.step_label)

    # Register a pending delay so tests can discover and release it.
    event = asyncio.Event()
    my_entry = {"event": event, "started_at": start}

    # If there's an existing delay for this key (e.g., from a pre-crash
    # orchestrator's dispatch that is still blocked), release it so it exits
    # quickly and doesn't interfere with this execution.
    old = _pending_delays.get(key)
    if old is not None:
        old["event"].set()
        old["superseded"] = True

    _pending_delays[key] = my_entry

    try:
        # Block until explicitly released via the delay-control API.
        await event.wait()
    finally:
        # Only remove our own entry — a newer execution may have replaced it.
        if _pending_delays.get(key) is my_entry:
            del _pending_delays[key]

    # If this execution was superseded by a newer one (post-recovery
    # re-dispatch), skip the tracker write to avoid confusing test assertions.
    if my_entry.get("superseded"):
        return DelayOutput(
            payload=input.payload,
            step_label=input.step_label,
            executed_at=datetime.now(timezone.utc).isoformat(),
            duration=time.monotonic() - start,
            attempt=context.attempt,
            tracker_attempt=0,
        )

    duration = time.monotonic() - start
    executed_at = datetime.now(timezone.utc).isoformat()

    if input.should_fail:
        raise RuntimeError(f"Intentional failure for step {input.step_label}")

    tracker_attempt = _count_tracker_records(run_id, input.step_label) + 1

    # Write tracker record
    record = {
        "run_id": run_id,
        "step_label": input.step_label,
        "executed_at": executed_at,
        "duration": duration,
        "pid": os.getpid(),
        "attempt": context.attempt,
        "tracker_attempt": tracker_attempt,
    }
    try:
        with open(TRACKER_FILE, "a") as f:
            f.write(json.dumps(record) + "\n")
    except OSError:
        pass  # Don't fail the step if tracker write fails

    return DelayOutput(
        payload=input.payload,
        step_label=input.step_label,
        executed_at=executed_at,
        duration=duration,
        attempt=context.attempt,
        tracker_attempt=tracker_attempt,
    )


# ---------------------------------------------------------------------------
# Delay-control HTTP API (separate from Stepflow protocol)
# ---------------------------------------------------------------------------


def _build_delay_control_app() -> FastAPI:
    """Build a plain FastAPI app with health check and delay-control endpoints.

    This does NOT include Stepflow JSON-RPC routes — the worker communicates
    with orchestrators via gRPC pull transport instead.
    """
    app = FastAPI()

    @app.get("/health")
    async def health():
        return JSONResponse({"status": "ok"})

    @app.get("/delay")
    async def get_delay(
        run_id: str | None = Query(None),
        step_id: str | None = Query(None),
    ):
        """Check whether a matching delay is currently pending."""
        match = _find_delay(run_id, step_id)
        if match is None:
            return JSONResponse({"delayed": False})
        _key, entry = match
        elapsed_ms = int((time.monotonic() - entry["started_at"]) * 1000)
        return JSONResponse({"delayed": True, "elapsed_ms": elapsed_ms})

    @app.post("/delay")
    async def post_delay(
        run_id: str | None = Query(None),
        step_id: str | None = Query(None),
    ):
        """Release a pending delay, allowing the step to complete."""
        match = _find_delay(run_id, step_id)
        if match is None:
            return JSONResponse(
                {"error": "no matching delay found"},
                status_code=404,
            )
        _key, entry = match
        entry["event"].set()
        return JSONResponse({"released": True})

    @app.post("/delay/release-all")
    async def post_release_all():
        """Release all pending delays at once."""
        count = 0
        for _key, entry in list(_pending_delays.items()):
            entry["event"].set()
            count += 1
        return JSONResponse({"released": count})

    return app


# ---------------------------------------------------------------------------
# gRPC pull worker loop with persistent reconnection
# ---------------------------------------------------------------------------


async def _grpc_loop(server: StepflowServer, url: str, queue_name: str):
    """Run a gRPC pull worker with persistent reconnection.

    When the orchestrator goes away (crash/restart), the worker exits
    cleanly and this loop restarts it after a short delay.
    """
    while True:
        try:
            logger.info("Connecting gRPC worker to %s (queue=%s)...", url, queue_name)
            await run_grpc_worker(server, url, queue_name, max_retries=15)
        except Exception:
            logger.exception("gRPC worker error for %s", url)
        logger.info("gRPC worker for %s exited, reconnecting in 2s...", url)
        await asyncio.sleep(2)


async def _run_http_server(app: FastAPI):
    """Run the delay-control HTTP server."""
    config = uvicorn.Config(
        app=app,
        host="0.0.0.0",
        port=8080,
        log_level="warning",
    )
    uv_server = uvicorn.Server(config)
    await uv_server.serve()


async def _main():
    app = _build_delay_control_app()

    # Run the delay-control HTTP server alongside dual gRPC pull workers
    # (one per orchestrator) for full multi-orchestrator test coverage.
    await asyncio.gather(
        _run_http_server(app),
        _grpc_loop(server, "orchestrator-1:7841", "test-worker"),
        _grpc_loop(server, "orchestrator-2:7842", "test-worker"),
    )


if __name__ == "__main__":
    asyncio.run(_main())
