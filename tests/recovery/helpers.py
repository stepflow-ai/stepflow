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

"""Shared helper functions for recovery integration tests."""

from __future__ import annotations

import asyncio
import json
import os
import subprocess
import time
from pathlib import Path

import httpx
import yaml

COMPOSE_FILE = str(Path(__file__).parent / "docker-compose.yml")
COMPOSE_OVERRIDE = os.environ.get("COMPOSE_OVERRIDE")

ORCH1_URL = "http://localhost:7841"
ORCH2_URL = "http://localhost:7842"

# Proto ExecutionStatus enum values (i32) used in gRPC/tonic-rest JSON responses.
STATUS_RUNNING = 1
STATUS_COMPLETED = 2
STATUS_FAILED = 3
STATUS_CANCELLED = 4
STATUS_RECOVERY_FAILED = 6

TERMINAL_STATUSES = {STATUS_COMPLETED, STATUS_FAILED, STATUS_CANCELLED, STATUS_RECOVERY_FAILED}

# Map integer status to human-readable names for assertions/logging.
STATUS_NAMES = {
    0: "unspecified",
    STATUS_RUNNING: "running",
    STATUS_COMPLETED: "completed",
    STATUS_FAILED: "failed",
    STATUS_CANCELLED: "cancelled",
    5: "paused",
    STATUS_RECOVERY_FAILED: "recoveryFailed",
}


def status_name(status_int: int) -> str:
    """Get human-readable name for a proto ExecutionStatus integer."""
    return STATUS_NAMES.get(status_int, f"unknown({status_int})")


# ---------------------------------------------------------------------------
# API helpers
# ---------------------------------------------------------------------------

async def store_flow(base_url: str, workflow_path: str) -> str:
    """Upload a workflow YAML file and return its flow_id."""
    with open(workflow_path) as f:
        flow = yaml.safe_load(f)

    async with httpx.AsyncClient(timeout=30) as client:
        resp = await client.post(
            f"{base_url}/api/v1/flows",
            json={"flow": flow},
        )
        resp.raise_for_status()
        data = resp.json()
        assert data.get("stored"), f"Flow not stored: {data}"
        return data["flowId"]


async def submit_run(base_url: str, flow_id: str, input_data: dict) -> str:
    """Submit a run with wait=false and return the run_id."""
    async with httpx.AsyncClient(timeout=30) as client:
        resp = await client.post(
            f"{base_url}/api/v1/runs",
            json={
                "flowId": flow_id,
                "input": [input_data],
            },
        )
        assert resp.status_code == 200, f"Expected 200, got {resp.status_code}: {resp.text}"
        return resp.json()["summary"]["runId"]


async def get_run(base_url: str, run_id: str) -> dict:
    """Get current run status (non-blocking).

    Returns a dict with summary fields including ``status`` (integer proto
    enum value), or ``None`` if the run is not found (HTTP 404).
    """
    async with httpx.AsyncClient(timeout=10) as client:
        resp = await client.get(f"{base_url}/api/v1/runs/{run_id}")
        if resp.status_code == 404:
            return None
        resp.raise_for_status()
        data = resp.json()
        # Flatten summary to top level for convenience.
        summary = data.get("summary", {})
        summary["steps"] = data.get("steps", [])
        return summary


async def wait_for_run(
    base_url: str,
    run_id: str,
    timeout: float = 90,
) -> dict:
    """Long-poll GET /runs/{id}?wait=true until terminal state or timeout.

    Falls back to polling if the long-poll connection is broken (e.g. after
    orchestrator restart).
    """
    deadline = time.monotonic() + timeout

    while time.monotonic() < deadline:
        remaining = max(1, int(deadline - time.monotonic()))
        try:
            async with httpx.AsyncClient(timeout=remaining + 5) as client:
                resp = await client.get(
                    f"{base_url}/api/v1/runs/{run_id}",
                    params={"wait": "true", "timeoutSecs": str(min(remaining, 30))},
                )
                if resp.status_code == 404:
                    await asyncio.sleep(2)
                    continue
                resp.raise_for_status()
                data = resp.json()
                summary = data.get("summary", {})
                status = summary.get("status")
                if status in TERMINAL_STATUSES:
                    summary["steps"] = data.get("steps", [])
                    return summary
        except (httpx.ConnectError, httpx.ReadError, httpx.ReadTimeout, httpx.RemoteProtocolError):
            await asyncio.sleep(2)
            continue

    raise TimeoutError(f"Run {run_id} did not reach terminal state within {timeout}s")


async def wait_for_run_on_either(
    run_id: str,
    timeout: float = 90,
) -> dict:
    """Poll both orchestrators until run completes."""
    deadline = time.monotonic() + timeout

    while time.monotonic() < deadline:
        for url in [ORCH1_URL, ORCH2_URL]:
            try:
                data = await get_run(url, run_id)
                if data is not None and data.get("status") in TERMINAL_STATUSES:
                    return data
            except Exception:
                pass
        await asyncio.sleep(2)

    raise TimeoutError(f"Run {run_id} did not complete within {timeout}s")


# ---------------------------------------------------------------------------
# Docker Compose helpers
# ---------------------------------------------------------------------------

def _compose(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    cmd = ["docker", "compose", "-f", COMPOSE_FILE]
    if COMPOSE_OVERRIDE:
        cmd.extend(["-f", str(Path(__file__).parent / COMPOSE_OVERRIDE)])
    cmd.extend(args)
    return subprocess.run(cmd, check=check, capture_output=True, text=True)


def docker_kill(*services: str):
    """Send SIGKILL to Docker Compose service(s).

    Note: Docker treats this as a manual stop — the restart policy does NOT
    trigger. Use crash_worker() for a realistic internal crash that Docker
    auto-restarts.
    """
    _compose("kill", *services)


def docker_start(*services: str):
    """Start stopped Docker Compose service(s)."""
    _compose("start", *services)


def crash_worker():
    """Crash the worker process from inside the container.

    Sends SIGUSR1 to PID 1 (the main Python process), which has a handler
    that calls os._exit(1). Because the process exits internally, Docker's
    ``restart: unless-stopped`` policy auto-restarts the container.
    """
    _compose(
        "exec", "-T", "worker",
        "python", "-c", "import os, signal; os.kill(1, signal.SIGUSR1)",
        check=False,  # the exec may fail if the container dies mid-command
    )


def compose_down():
    """Tear down compose environment and remove volumes."""
    _compose("down", "-v", "--timeout", "5", check=False)


def compose_up(retries: int = 1):
    """Start all services from pre-built images.

    Images must be built beforehand (e.g. via ``docker compose build`` in
    check-recovery.sh).  This only creates/starts containers and waits for
    health checks.

    Retries with a full teardown between attempts to handle transient Docker
    health-check races (e.g. a service reports unhealthy before Postgres
    accepts connections).
    """
    for attempt in range(1 + retries):
        result = _compose(
            "up", "-d", "--force-recreate",
            "--wait", "--wait-timeout", "60",
            check=False,
        )
        if result.returncode == 0:
            return
        # Log stderr so CI/local runs can see why startup failed.
        msg = f"[compose_up] attempt {attempt + 1} failed (rc={result.returncode})"
        if result.stderr:
            msg += f"\n[compose_up] stderr:\n{result.stderr[-2000:]}"
        print(msg, flush=True)
        if attempt < retries:
            print("[compose_up] retrying after full teardown...", flush=True)
            compose_down()
    # Final attempt failed — raise so the test reports an error.
    raise subprocess.CalledProcessError(
        result.returncode, result.args, result.stdout, result.stderr,
    )


def wait_for_health(url: str, timeout: float = 60):
    """Poll health endpoint until healthy or timeout."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            resp = httpx.get(f"{url}/api/v1/health", timeout=2)
            if resp.status_code == 200:
                return
        except (httpx.ConnectError, httpx.ReadError, httpx.ReadTimeout, httpx.RemoteProtocolError):
            pass
        time.sleep(1)
    raise TimeoutError(f"Service at {url} not healthy within {timeout}s")


# ---------------------------------------------------------------------------
# Execution tracker helpers
# ---------------------------------------------------------------------------

def read_tracker_records() -> list[dict]:
    """Read execution tracker records from the worker container."""
    result = _compose(
        "exec", "-T", "worker", "cat", "/tracker/executions.jsonl",
        check=False,
    )
    if result.returncode != 0 or not result.stdout.strip():
        return []

    records = []
    for line in result.stdout.strip().split("\n"):
        line = line.strip()
        if line:
            records.append(json.loads(line))
    return records


def count_step_executions(
    records: list[dict],
    step_label: str,
    run_id: str | None = None,
) -> int:
    """Count how many times a specific step_label appears in tracker records.

    If run_id is provided, only count records matching both run_id and step_label.
    """
    return sum(
        1
        for r in records
        if r.get("step_label") == step_label
        and (run_id is None or r.get("run_id") == run_id)
    )


def get_step_tracker_records(
    records: list[dict],
    step_label: str,
    run_id: str | None = None,
) -> list[dict]:
    """Get tracker records for a specific step_label (and optionally run_id)."""
    return [
        r
        for r in records
        if r.get("step_label") == step_label
        and (run_id is None or r.get("run_id") == run_id)
    ]


def clear_tracker():
    """Clear the execution tracker file."""
    _compose(
        "exec", "-T", "worker", "sh", "-c", "> /tracker/executions.jsonl",
        check=False,
    )


# ---------------------------------------------------------------------------
# Checkpoint verification helpers
# ---------------------------------------------------------------------------

def docker_logs(service: str) -> str:
    """Get the stdout/stderr logs for a Docker Compose service."""
    result = _compose("logs", "--no-log-prefix", service, check=False)
    return result.stdout + result.stderr


def get_orchestrator_logs(service: str = "orchestrator-1") -> str:
    """Get the stdout/stderr logs for an orchestrator service."""
    return docker_logs(service)


def has_checkpoint_created_log(service: str = "orchestrator-1") -> bool:
    """Check if the orchestrator logged creating a checkpoint during execution."""
    return "Checkpoint created for run" in get_orchestrator_logs(service)


def has_checkpoint_recovery_log(service: str = "orchestrator-1") -> bool:
    """Check if the orchestrator logged restoring from a checkpoint during recovery."""
    return "Restoring from checkpoint" in get_orchestrator_logs(service)


def assert_checkpoints_used_in_recovery(*services: str):
    """Assert that at least one orchestrator created AND restored checkpoints.

    This verifies the full checkpoint lifecycle:
    1. Checkpoints were created during execution (before crash)
    2. Recovery loaded a checkpoint (after restart/failover)

    Fetches logs once per service to avoid redundant docker-compose calls.
    """
    # Fetch logs once per service to avoid redundant docker-compose calls
    all_logs = {s: get_orchestrator_logs(s) for s in services}
    created = any("Checkpoint created for run" in logs for logs in all_logs.values())
    restored = any("Restoring from checkpoint" in logs for logs in all_logs.values())
    assert created, (
        f"Expected checkpoint creation log in {services}, "
        "but no 'Checkpoint created' message found"
    )
    assert restored, (
        f"Expected checkpoint recovery log in {services}, "
        "but no 'Restoring from checkpoint' message found"
    )


def wait_for_worker_health(timeout: float = 30):
    """Wait for the worker container to become healthy after restart."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            resp = httpx.get("http://localhost:8080/health", timeout=2)
            if resp.status_code == 200:
                return
        except (httpx.ConnectError, httpx.ReadError, httpx.ReadTimeout, httpx.RemoteProtocolError):
            pass
        time.sleep(1)
    raise TimeoutError(f"Worker not healthy within {timeout}s")


# ---------------------------------------------------------------------------
# Delay-control helpers
# ---------------------------------------------------------------------------

WORKER_URL = "http://localhost:8080"


def poll_for_delay(
    step_id: str,
    run_id: str | None = None,
    timeout: float = 30,
) -> dict:
    """Poll the worker's delay-control API until the given step is delayed.

    Returns the response dict (``{"delayed": True, "elapsed_ms": N}``).
    Raises ``TimeoutError`` if the delay does not appear within *timeout*
    seconds.
    """
    params: dict[str, str] = {"step_id": step_id}
    if run_id is not None:
        params["run_id"] = run_id

    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            resp = httpx.get(f"{WORKER_URL}/delay", params=params, timeout=2)
            if resp.status_code == 200:
                data = resp.json()
                if data.get("delayed"):
                    return data
        except (httpx.ConnectError, httpx.ReadError, httpx.ReadTimeout, httpx.RemoteProtocolError):
            pass
        time.sleep(0.5)
    raise TimeoutError(f"Delay for step_id={step_id!r} did not appear within {timeout}s")


def release_delay(
    step_id: str,
    run_id: str | None = None,
) -> bool:
    """Release a pending delay via the worker's delay-control API.

    Returns ``True`` if the delay was found and released (HTTP 200),
    ``False`` if no matching delay was found (HTTP 404).
    """
    params: dict[str, str] = {"step_id": step_id}
    if run_id is not None:
        params["run_id"] = run_id

    resp = httpx.post(f"{WORKER_URL}/delay", params=params, timeout=5)
    if resp.status_code == 200:
        return True
    if resp.status_code == 404:
        return False
    resp.raise_for_status()
    return False  # unreachable, but keeps mypy happy


def release_all_delays() -> int:
    """Release all pending delays via the worker's delay-control API.

    Use this after killing an orchestrator (but before restarting) to clear
    stale pre-crash delay entries from the worker's registry. Recovery
    re-dispatches will create fresh entries that tests can then poll and
    release deterministically.

    Returns the number of delays released, or 0 if the worker is unreachable.
    """
    try:
        resp = httpx.post(f"{WORKER_URL}/delay/release-all", timeout=5)
        return resp.json().get("released", 0)
    except (httpx.ConnectError, httpx.ReadError, httpx.ReadTimeout, httpx.RemoteProtocolError):
        return 0


def query_run_status(run_id: str) -> int | None:
    """Query run status from either orchestrator (sync, best-effort).

    Returns the proto ExecutionStatus integer, or None if unreachable.
    """
    for url in [ORCH1_URL, ORCH2_URL]:
        try:
            resp = httpx.get(f"{url}/api/v1/runs/{run_id}", timeout=5)
            if resp.status_code == 200:
                return resp.json().get("summary", {}).get("status")
        except Exception:
            continue
    return None
