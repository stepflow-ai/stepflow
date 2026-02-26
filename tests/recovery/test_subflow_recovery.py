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

"""Subflow recovery tests.

These tests verify that when an orchestrator crashes while a parent step is
waiting for a subflow, recovery resumes the subflow in-place rather than
restarting it from scratch. This is achieved via SubflowSubmitted journal
events and deterministic subflow key deduplication.

The test workflow (subflow_delay.yaml) has:
  store_subflow (instant) -> run_subflow (8s inner delay) -> final_step (3s)

Tests use the delay-control API to hold delays until the crash/recovery
sequence is complete, then release them — eliminating timing assumptions.
"""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest

from helpers import (
    ORCH1_URL,
    ORCH2_URL,
    clear_tracker,
    count_step_executions,
    docker_kill,
    docker_start,
    get_step_tracker_records,
    poll_for_delay,
    read_tracker_records,
    release_delay,
    store_flow,
    submit_run,
    wait_for_health,
    wait_for_run,
    wait_for_run_on_either,
)

WORKFLOWS = Path(__file__).parent / "workflows"


@pytest.mark.asyncio
async def test_subflow_restart_recovery(compose_env):
    """Kill orch-1 while subflow is running, restart it, verify subflow is not re-executed.

    Scenario:
    1. Submit the subflow_delay workflow to orch-1
    2. Wait for inner_delay to be held (subflow is in-flight)
    3. Kill orch-1
    4. Restart orch-1 (same ID -> startup recovery)
    5. Release inner_delay so the step can complete
    6. Wait for run to complete
    7. Assert: inner_delay executed exactly once (recovered, not restarted)
    8. Assert: final_step executed once
    """
    clear_tracker()

    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "subflow_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "subflow_test"}})

    # Wait for inner_delay to be actively held (deterministic — no timing guesses)
    poll_for_delay("inner_delay", timeout=20)

    # Kill orchestrator-1 while the delay is held
    docker_kill("orchestrator-1")

    # Restart orchestrator-1 (same container, same orch-1 ID)
    docker_start("orchestrator-1")
    wait_for_health(ORCH1_URL, timeout=30)

    # Release the delay so the step can complete after recovery
    release_delay("inner_delay")

    # Wait for run to complete via recovery
    result = await wait_for_run(ORCH1_URL, run_id, timeout=90)

    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    records = read_tracker_records()

    # Key assertion: inner_delay should have executed exactly once.
    # Without subflow recovery, it would execute twice (once before crash, once after).
    # Note: inner_delay runs in the subflow (different run_id), so don't filter by parent run_id.
    assert count_step_executions(records, "inner_delay") == 1, (
        "inner_delay should execute exactly once — subflow recovery should prevent re-execution"
    )

    # final_step runs in the parent flow so its run_id matches
    assert count_step_executions(records, "final_step", run_id) >= 1, (
        "final_step should have executed after recovery"
    )


@pytest.mark.asyncio
async def test_subflow_failover_recovery(compose_env):
    """Kill orch-1 permanently while subflow is running, let orch-2 recover.

    Scenario:
    1. Submit the subflow_delay workflow to orch-1
    2. Wait for inner_delay to be held
    3. Kill orch-1 permanently (don't restart)
    4. Release orch-1's in-flight delay, then wait for orch-2's re-dispatch
    5. Release recovered inner_delay and final_step
    6. Assert: all inner_delay executions belong to a single subflow run_id
       (recovery reuses the subflow, doesn't restart from scratch)
    7. Assert: final_step executed at least once
    """
    clear_tracker()

    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "subflow_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "failover_test"}})

    # Wait for inner_delay to be actively held
    poll_for_delay("inner_delay", timeout=20)

    # Kill orchestrator-1 permanently
    docker_kill("orchestrator-1")

    # Release orch-1's in-flight delay so it clears from the worker.
    # (The response is lost since orch-1 is dead, but a tracker record is written.)
    release_delay("inner_delay")

    # Orch-2 recovers via etcd lease expiry (~6s TTL) and re-dispatches
    # inner_delay. Since the old delay was released and removed, this creates
    # a fresh delay entry. Poll until it appears, then release it.
    poll_for_delay("inner_delay", timeout=30)
    release_delay("inner_delay")

    # Release final_step when dispatched
    poll_for_delay("final_step", timeout=30)
    release_delay("final_step")

    # Wait for run to complete on either orchestrator
    result = await wait_for_run_on_either(run_id, timeout=90)

    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    records = read_tracker_records()

    # At-least-once dispatch means inner_delay may execute more than once within
    # the recovered subflow (orch-2 re-dispatches in-flight steps). The key
    # assertion is that all executions belong to a SINGLE subflow run_id —
    # proving the subflow was recovered, not restarted from scratch.
    inner_records = get_step_tracker_records(records, "inner_delay")
    assert len(inner_records) >= 1, "inner_delay should have executed at least once"
    distinct_run_ids = set(r["run_id"] for r in inner_records)
    assert len(distinct_run_ids) == 1, (
        "inner_delay should run in exactly one subflow — recovery should prevent "
        f"duplicate subflows. Got run_ids: {distinct_run_ids}"
    )

    assert count_step_executions(records, "final_step", run_id) >= 1, (
        "final_step should have executed after recovery"
    )


@pytest.mark.asyncio
async def test_completed_subflow_not_restarted(compose_env):
    """Kill orch-1 after inner subflow completes but during final_step, verify no re-execution.

    Scenario:
    1. Submit the subflow_delay workflow to orch-1
    2. Wait for inner_delay to be held, then release it immediately
    3. Wait for final_step to be held (inner subflow is now complete)
    4. Kill orch-1 while final_step is held
    5. Restart orch-1
    6. Release final_step
    7. Assert: inner_delay count is exactly 1 (completed subflow not restarted)
    """
    clear_tracker()

    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "subflow_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "completed_subflow"}})

    # Wait for inner_delay to be held, then release it so the subflow completes
    poll_for_delay("inner_delay", timeout=20)
    release_delay("inner_delay")

    # Wait for final_step to be held (inner subflow done, orchestrator dispatched final_step)
    poll_for_delay("final_step", timeout=30)

    # Kill orchestrator-1 while final_step is held
    docker_kill("orchestrator-1")

    # Restart and recover
    docker_start("orchestrator-1")
    wait_for_health(ORCH1_URL, timeout=30)

    # Release final_step so it can complete after recovery
    release_delay("final_step")

    result = await wait_for_run(ORCH1_URL, run_id, timeout=90)
    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    records = read_tracker_records()

    # The inner subflow completed before the crash — it must not be re-executed
    # inner_delay runs in the subflow (different run_id from parent)
    assert count_step_executions(records, "inner_delay") == 1, (
        "inner_delay should not be re-executed — subflow was already complete"
    )

    # final_step may have been re-executed (was in-flight during crash, at-least-once)
    assert count_step_executions(records, "final_step", run_id) >= 1, (
        "final_step should have executed"
    )
