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
Total: ~11s. We kill the orchestrator while the inner subflow is running.
"""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest

from helpers import (
    ORCH1_URL,
    ORCH2_URL,
    count_step_executions,
    docker_kill,
    docker_start,
    poll_tracker_for_step,
    read_tracker_records,
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
    2. Wait for inner_delay to start (subflow is in-flight)
    3. Kill orch-1
    4. Restart orch-1 (same ID -> startup recovery)
    5. Wait for run to complete
    6. Assert: inner_delay executed exactly once (recovered, not restarted)
    7. Assert: final_step executed once
    """
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "subflow_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "subflow_test"}})

    # Wait for the inner subflow to start executing
    assert poll_tracker_for_step("inner_delay", timeout=20), (
        "inner_delay did not start in time"
    )

    # Verify the flow is still in-progress: final_step must NOT have executed yet
    pre_kill_records = read_tracker_records()
    assert count_step_executions(pre_kill_records, "final_step") == 0, (
        "final_step should not have executed before kill"
    )

    # Kill orchestrator-1
    docker_kill("orchestrator-1")

    # Restart orchestrator-1 (same container, same orch-1 ID)
    docker_start("orchestrator-1")
    wait_for_health(ORCH1_URL, timeout=30)

    # Wait for run to complete via recovery
    result = await wait_for_run(ORCH1_URL, run_id, timeout=90)

    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    records = read_tracker_records()

    # Key assertion: inner_delay should have executed exactly once.
    # Without subflow recovery, it would execute twice (once before crash, once after).
    assert count_step_executions(records, "inner_delay", run_id) == 1, (
        "inner_delay should execute exactly once — subflow recovery should prevent re-execution"
    )

    # final_step should have executed once after recovery
    assert count_step_executions(records, "final_step", run_id) >= 1, (
        "final_step should have executed after recovery"
    )


@pytest.mark.asyncio
async def test_subflow_failover_recovery(compose_env):
    """Kill orch-1 permanently while subflow is running, let orch-2 recover.

    Scenario:
    1. Submit the subflow_delay workflow to orch-1
    2. Wait for inner_delay to start
    3. Kill orch-1 permanently (don't restart)
    4. Wait for orch-2 to pick up recovery via etcd lease expiry
    5. Assert: inner_delay executed exactly once
    6. Assert: final_step executed once
    """
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "subflow_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "failover_test"}})

    # Wait for the inner subflow to start executing
    assert poll_tracker_for_step("inner_delay", timeout=20), (
        "inner_delay did not start in time"
    )

    # Verify still in-progress
    pre_kill_records = read_tracker_records()
    assert count_step_executions(pre_kill_records, "final_step") == 0, (
        "final_step should not have executed before kill"
    )

    # Kill orchestrator-1 permanently
    docker_kill("orchestrator-1")

    # Wait for run to complete on either orchestrator (orch-2 will recover via
    # etcd lease expiry, which takes ~6s based on leaseTtlSecs config)
    result = await wait_for_run_on_either(run_id, timeout=90)

    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    records = read_tracker_records()

    # inner_delay should execute exactly once (subflow recovery prevents restart)
    assert count_step_executions(records, "inner_delay", run_id) == 1, (
        "inner_delay should execute exactly once — subflow recovery should prevent re-execution"
    )

    assert count_step_executions(records, "final_step", run_id) >= 1, (
        "final_step should have executed after recovery"
    )


@pytest.mark.asyncio
async def test_completed_subflow_not_restarted(compose_env):
    """Kill orch-1 after inner subflow completes but before final_step, verify no re-execution.

    Scenario:
    1. Submit the subflow_delay workflow to orch-1
    2. Wait for inner_delay to complete (8s)
    3. Kill orch-1 before final_step finishes (3s window)
    4. Restart orch-1
    5. Assert: inner_delay count is exactly 1 (completed subflow not restarted)
    """
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "subflow_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "completed_subflow"}})

    # Wait for inner_delay to complete. It takes 8s.
    # Poll until inner_delay appears in tracker, then wait for it to finish.
    assert poll_tracker_for_step("inner_delay", timeout=20), (
        "inner_delay did not start in time"
    )

    # Wait for final_step to start (inner_delay done, run_subflow returning result,
    # then final_step dispatched). We need to kill between final_step start and end.
    assert poll_tracker_for_step("final_step", timeout=30), (
        "final_step did not start in time"
    )

    # Kill immediately — final_step takes 3s so we're likely still in it
    docker_kill("orchestrator-1")

    # Restart and recover
    docker_start("orchestrator-1")
    wait_for_health(ORCH1_URL, timeout=30)

    result = await wait_for_run(ORCH1_URL, run_id, timeout=90)
    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    records = read_tracker_records()

    # The inner subflow completed before the crash — it must not be re-executed
    assert count_step_executions(records, "inner_delay", run_id) == 1, (
        "inner_delay should not be re-executed — subflow was already complete"
    )

    # final_step may have been re-executed (was in-flight during crash, at-least-once)
    assert count_step_executions(records, "final_step", run_id) >= 1, (
        "final_step should have executed"
    )
