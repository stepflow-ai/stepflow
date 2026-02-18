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
    poll_tracker_for_step,
    read_tracker_records,
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

    # 2. Wait for step1 to complete
    assert poll_tracker_for_step("step1", timeout=15), "step1 did not complete in time"

    # 3. Verify flow is still in-progress: step3 must NOT have executed yet.
    pre_kill_records = read_tracker_records()
    assert count_step_executions(pre_kill_records, "step3") == 0, (
        "step3 should not have executed before kill — flow should still be in-progress"
    )

    # 4. Kill orchestrator-1 (stays dead)
    docker_kill("orchestrator-1")

    # 5. Wait for lease TTL expiry (6s) + orphan detection (~3s polling)
    #    etcd push-based watch_orphans should detect almost immediately after
    #    the lease expires, but give extra margin.
    await asyncio.sleep(10)

    # 6. Orch-2 should claim the orphan and finish the run
    result = await wait_for_run(ORCH2_URL, run_id, timeout=60)

    # 7. Assertions
    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    records = read_tracker_records()

    # Verify recovery produced new work: step3 must now be present
    assert count_step_executions(records, "step3") >= 1, (
        "step3 should have been executed by recovery — failover did not produce new work"
    )
    # step1 completed and was journaled before the kill — must not re-execute
    assert count_step_executions(records, "step1") == 1, (
        "step1 should not be re-executed during failover recovery"
    )
    # In-flight steps may be re-executed (at-least-once semantics)
    assert count_step_executions(records, "step2") >= 1, "step2 should have executed"


@pytest.mark.asyncio
async def test_failover_recovery_parallel(compose_env):
    """Kill orch-1 during parallel execution, orch-2 picks up."""
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "parallel_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "failover_parallel"}})

    # parallel_a (3s) should complete before we kill
    assert poll_tracker_for_step("parallel_a", timeout=15), "parallel_a did not complete in time"

    # Verify flow is still in-progress: aggregate can't have run yet
    pre_kill_records = read_tracker_records()
    assert count_step_executions(pre_kill_records, "aggregate") == 0, (
        "aggregate should not have executed before kill — flow should still be in-progress"
    )

    docker_kill("orchestrator-1")
    await asyncio.sleep(10)

    result = await wait_for_run(ORCH2_URL, run_id, timeout=90)
    assert result["status"] == "completed"

    records = read_tracker_records()

    # Verify recovery produced new work: aggregate must now be present
    assert count_step_executions(records, "aggregate") >= 1, (
        "aggregate should have been executed by recovery — failover did not produce new work"
    )
    # All parallel steps must have executed at least once (at-least-once semantics)
    for label in ["parallel_a", "parallel_b", "parallel_c", "parallel_d"]:
        assert count_step_executions(records, label) >= 1, f"{label} should have executed"
