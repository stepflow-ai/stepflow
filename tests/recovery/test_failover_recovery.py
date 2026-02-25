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

"""Scenario B: Orchestrator killed and stays dead.

The surviving orchestrator should detect the orphaned run via etcd lease
expiry, claim it, replay the journal, and resume execution. Steps whose
completion was durably journaled before the crash must not be
re-executed. In-flight steps may be re-executed (at-least-once).

Tests use the delay-control API to hold delays and crash at a
deterministic point, eliminating timing assumptions.
"""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest

from helpers import (
    ORCH1_URL,
    ORCH2_URL,
    assert_checkpoints_used_in_recovery,
    count_step_executions,
    docker_kill,
    get_step_tracker_records,
    has_checkpoint_created_log,
    has_checkpoint_recovery_log,
    poll_for_delay,
    read_tracker_records,
    release_delay,
    store_flow,
    submit_run,
    wait_for_run,
)

WORKFLOWS = Path(__file__).parent / "workflows"


@pytest.mark.asyncio
async def test_failover_recovery_sequential(compose_env):
    """Kill orch-1 and leave it dead. Orch-2 claims orphan after lease TTL."""
    # 1. Upload workflow and submit to orch-1
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "sequential_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": 99}})

    # 2. Wait for step1 to be held, release it so it completes
    poll_for_delay("step1", timeout=15)
    release_delay("step1")

    # 3. Wait for step2 to be held — step1 is now durably complete
    poll_for_delay("step2", timeout=15)

    # 4. Kill orchestrator-1 (stays dead) while step2 is deterministically held
    docker_kill("orchestrator-1")

    # 5. Release step2 so orch-2 can proceed after claiming the orphan
    release_delay("step2")

    # 6. Orch-2 should claim the orphan after lease expiry and finish the run
    result = await wait_for_run(ORCH2_URL, run_id, timeout=60)

    # 7. Assertions
    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    records = read_tracker_records()

    # Verify recovery produced new work: step3 must now be present
    assert count_step_executions(records, "step3", run_id) >= 1, (
        "step3 should have been executed by recovery — failover did not produce new work"
    )
    # step1 completed and was journaled before the kill — must not re-execute
    assert count_step_executions(records, "step1", run_id) == 1, (
        "step1 should not be re-executed during failover recovery"
    )
    # In-flight steps may be re-executed (at-least-once semantics)
    assert count_step_executions(records, "step2", run_id) >= 1, "step2 should have executed"

    # Attempt tracking: step1 completed before crash, not retried.
    step1_records = get_step_tracker_records(records, "step1", run_id)
    assert step1_records[0]["attempt"] == 1, "step1 should not have been retried"
    assert step1_records[0]["tracker_attempt"] == 1, "step1 should have a single execution"

    # step3 dispatched fresh by the failover orchestrator.
    step3_records = get_step_tracker_records(records, "step3", run_id)
    assert step3_records[-1]["attempt"] == 1, "step3 should be attempt 1 (fresh dispatch)"
    assert step3_records[-1]["tracker_attempt"] == 1, "step3 should have a single execution"

    # Checkpoint verification: orch-1 created checkpoints before crash,
    # orch-2 restored from a checkpoint during failover recovery.
    assert has_checkpoint_created_log("orchestrator-1"), (
        "orch-1 should have created checkpoints before crash"
    )
    assert has_checkpoint_recovery_log("orchestrator-2"), (
        "orch-2 should have restored from checkpoint during failover recovery"
    )


@pytest.mark.asyncio
async def test_failover_recovery_parallel(compose_env):
    """Kill orch-1 during parallel execution, orch-2 picks up."""
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "parallel_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "failover_parallel"}})

    # Wait for parallel_a to be held, release it so it completes
    poll_for_delay("parallel_a", timeout=15)
    release_delay("parallel_a")

    # Wait for parallel_d (longest) to be held — all parallel steps are in-flight
    poll_for_delay("parallel_d", timeout=15)

    # Kill orchestrator-1 (stays dead)
    docker_kill("orchestrator-1")

    # Release remaining parallel steps so orch-2 can proceed after failover
    for label in ["parallel_b", "parallel_c", "parallel_d"]:
        release_delay(label)

    result = await wait_for_run(ORCH2_URL, run_id, timeout=90)
    assert result["status"] == "completed"

    records = read_tracker_records()

    # Verify recovery produced new work: aggregate must now be present
    assert count_step_executions(records, "aggregate", run_id) >= 1, (
        "aggregate should have been executed by recovery — failover did not produce new work"
    )
    # All parallel steps must have executed at least once (at-least-once semantics)
    for label in ["parallel_a", "parallel_b", "parallel_c", "parallel_d"]:
        assert count_step_executions(records, label, run_id) >= 1, f"{label} should have executed"

    # Attempt tracking: aggregate dispatched fresh by the failover orchestrator.
    agg_records = get_step_tracker_records(records, "aggregate", run_id)
    assert agg_records[-1]["attempt"] == 1, "aggregate should be attempt 1 (fresh dispatch)"
    assert agg_records[-1]["tracker_attempt"] == 1, "aggregate should have a single execution"
