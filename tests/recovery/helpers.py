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
import subprocess
import time
from pathlib import Path

import httpx
import yaml

COMPOSE_FILE = str(Path(__file__).parent / "docker-compose.yml")

ORCH1_URL = "http://localhost:7841"
ORCH2_URL = "http://localhost:7842"


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
    """Submit a run with wait=false and return the run_id (202 Accepted)."""
    async with httpx.AsyncClient(timeout=30) as client:
        resp = await client.post(
            f"{base_url}/api/v1/runs",
            json={
                "flowId": flow_id,
                "input": [input_data],
            },
        )
        assert resp.status_code == 202, f"Expected 202, got {resp.status_code}: {resp.text}"
        return resp.json()["runId"]


async def get_run(base_url: str, run_id: str) -> dict:
    """Get current run status (non-blocking)."""
    async with httpx.AsyncClient(timeout=10) as client:
        resp = await client.get(f"{base_url}/api/v1/runs/{run_id}")
        if resp.status_code == 404:
            return {"status": "not_found"}
        resp.raise_for_status()
        return resp.json()


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
    terminal = {"completed", "failed", "cancelled", "recoveryFailed"}

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
                if data.get("status") in terminal:
                    return data
        except (httpx.ConnectError, httpx.ReadTimeout, httpx.RemoteProtocolError):
            await asyncio.sleep(2)
            continue

    raise TimeoutError(f"Run {run_id} did not reach terminal state within {timeout}s")


async def wait_for_run_on_either(
    run_id: str,
    timeout: float = 90,
) -> dict:
    """Poll both orchestrators until run completes."""
    deadline = time.monotonic() + timeout
    terminal = {"completed", "failed", "cancelled", "recoveryFailed"}

    while time.monotonic() < deadline:
        for url in [ORCH1_URL, ORCH2_URL]:
            try:
                data = await get_run(url, run_id)
                if data.get("status") in terminal:
                    return data
            except Exception:
                pass
        await asyncio.sleep(2)

    raise TimeoutError(f"Run {run_id} did not complete within {timeout}s")


# ---------------------------------------------------------------------------
# Docker Compose helpers
# ---------------------------------------------------------------------------

def _compose(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["docker", "compose", "-f", COMPOSE_FILE, *args],
        check=check,
        capture_output=True,
        text=True,
    )


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


def compose_up():
    """Build and start all services."""
    _compose("up", "-d", "--build", "--wait", check=True)


def wait_for_health(url: str, timeout: float = 60):
    """Poll health endpoint until healthy or timeout."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            resp = httpx.get(f"{url}/api/v1/health", timeout=2)
            if resp.status_code == 200:
                return
        except (httpx.ConnectError, httpx.ReadTimeout, httpx.RemoteProtocolError):
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


def poll_tracker_for_step(
    step_label: str,
    run_id: str | None = None,
    timeout: float = 30,
) -> bool:
    """Poll tracker until a given step_label appears, or timeout."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        records = read_tracker_records()
        if count_step_executions(records, step_label, run_id=run_id) > 0:
            return True
        time.sleep(1)
    return False


def wait_for_worker_health(timeout: float = 30):
    """Wait for the worker container to become healthy after restart."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            resp = httpx.get("http://localhost:8080/health", timeout=2)
            if resp.status_code == 200:
                return
        except (httpx.ConnectError, httpx.ReadTimeout, httpx.RemoteProtocolError):
            pass
        time.sleep(1)
    raise TimeoutError(f"Worker not healthy within {timeout}s")
