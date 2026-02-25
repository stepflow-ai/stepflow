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

"""Scenario A: Orchestrator killed and restarted (same ID).

The restarted orchestrator should recover its own orphaned runs via
startup recovery and journal replay. Steps whose completion was durably
journaled before the crash must not be re-executed. Steps that were
in-flight (dispatched to the worker but result not yet received by the
orchestrator) may be re-executed — this is correct at-least-once
semantics.
"""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest

from helpers import (
    ORCH1_URL,
    assert_checkpoints_used_in_recovery,
    count_step_executions,
    docker_kill,
    docker_start,
    get_step_tracker_records,
    poll_tracker_for_step,
    read_tracker_records,
    store_flow,
    submit_run,
    wait_for_health,
    wait_for_run,
)

WORKFLOWS = Path(__file__).parent / "workflows"


@pytest.mark.asyncio
async def test_restart_recovery_sequential(compose_env):
    """Kill orch-1 after step1, restart it, verify recovery completes the run."""
    # 1. Upload workflow and submit run (non-blocking)
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "sequential_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": 42}})

    # 2. Wait for step1 to complete (~5s delay + margin)
    assert poll_tracker_for_step("step1", timeout=15), "step1 did not complete in time"

    # 3. Verify the flow is still in-progress: step3 must NOT have executed yet.
    #    Sequential: step1(5s) -> step2(5s) -> step3(5s). After step1 finishes
    #    (≥5s), step2 is starting, step3 can't have started.
    pre_kill_records = read_tracker_records()
    assert count_step_executions(pre_kill_records, "step3") == 0, (
        "step3 should not have executed before kill — flow should still be in-progress"
    )

    # 4. Kill orchestrator-1 (SIGKILL — simulates crash)
    docker_kill("orchestrator-1")

    # 5. Restart orchestrator-1 (same container, same orch-1 ID)
    docker_start("orchestrator-1")
    wait_for_health(ORCH1_URL, timeout=30)

    # 6. Wait for run to complete via recovery
    result = await wait_for_run(ORCH1_URL, run_id, timeout=60)

    # 7. Assertions
    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    records = read_tracker_records()

    # Verify recovery produced new work: step3 must now be present
    assert count_step_executions(records, "step3", run_id) >= 1, (
        "step3 should have been executed by recovery — recovery did not produce new work"
    )
    # step1 completed and was journaled before the kill — must not re-execute
    assert count_step_executions(records, "step1", run_id) == 1, (
        "step1 should not be re-executed after recovery"
    )
    # step2 may have been in-flight when killed (at-least-once semantics)
    assert count_step_executions(records, "step2", run_id) >= 1, "step2 should have executed"

    # Attempt tracking: step1 was completed before the crash, so it should
    # show attempt=1 (no retries needed) and tracker_attempt=1 (single execution).
    step1_records = get_step_tracker_records(records, "step1", run_id)
    assert step1_records[0]["attempt"] == 1, "step1 should not have been retried"
    assert step1_records[0]["tracker_attempt"] == 1, "step1 should have a single execution"

    # step3 was dispatched fresh by the recovered orchestrator.
    # It was never started before the crash, so attempt=1 is correct.
    step3_records = get_step_tracker_records(records, "step3", run_id)
    assert step3_records[-1]["attempt"] == 1, "step3 should be attempt 1 (fresh dispatch)"
    assert step3_records[-1]["tracker_attempt"] == 1, "step3 should have a single execution"

    # step2 was in-flight when the orchestrator crashed. After recovery,
    # the journal replay sees TasksStarted(attempt=1) with no TaskCompleted,
    # so the re-dispatch should be attempt=2.
    step2_records = get_step_tracker_records(records, "step2", run_id)
    if len(step2_records) > 1:
        # step2 was re-executed after recovery — the recovery attempt should be >= 2
        assert step2_records[-1]["attempt"] >= 2, (
            "step2 re-execution after recovery should show attempt >= 2"
        )

    # Checkpoint verification: orch-1 should have created checkpoints during
    # execution and restored from one during recovery.
    assert_checkpoints_used_in_recovery("orchestrator-1")


@pytest.mark.asyncio
async def test_restart_recovery_parallel(compose_env):
    """Kill orch-1 during parallel execution, restart, verify recovery."""
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "parallel_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "parallel_test"}})

    # parallel_a completes in ~3s, others take longer
    assert poll_tracker_for_step("parallel_a", timeout=15), "parallel_a did not complete in time"

    # Verify flow is still in-progress: aggregate depends on all parallel steps,
    # and parallel_d takes 7s, so aggregate can't have run after only ~3s.
    pre_kill_records = read_tracker_records()
    assert count_step_executions(pre_kill_records, "aggregate") == 0, (
        "aggregate should not have executed before kill — flow should still be in-progress"
    )

    docker_kill("orchestrator-1")
    docker_start("orchestrator-1")
    wait_for_health(ORCH1_URL, timeout=30)

    result = await wait_for_run(ORCH1_URL, run_id, timeout=90)
    assert result["status"] == "completed"

    records = read_tracker_records()

    # Verify recovery produced new work: aggregate must now be present
    assert count_step_executions(records, "aggregate", run_id) >= 1, (
        "aggregate should have been executed by recovery — recovery did not produce new work"
    )
    # All parallel steps must have executed at least once (at-least-once semantics).
    for label in ["parallel_a", "parallel_b", "parallel_c", "parallel_d"]:
        assert count_step_executions(records, label, run_id) >= 1, f"{label} should have executed"

    # Attempt tracking: aggregate was dispatched fresh by the recovered orchestrator.
    agg_records = get_step_tracker_records(records, "aggregate", run_id)
    assert agg_records[-1]["attempt"] == 1, "aggregate should be attempt 1 (fresh dispatch)"
    assert agg_records[-1]["tracker_attempt"] == 1, "aggregate should have a single execution"
