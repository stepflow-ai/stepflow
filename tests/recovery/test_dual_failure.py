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

"""Scenario C: Both orchestrators killed and eventually restarted.

After restart, each orchestrator should recover runs without duplication.
The etcd lease manager ensures no split-brain (each run is claimed by
exactly one orchestrator).
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
    get_step_tracker_records,
    poll_tracker_for_step,
    read_tracker_records,
    store_flow,
    submit_run,
    wait_for_health,
    wait_for_run_on_either,
)

WORKFLOWS = Path(__file__).parent / "workflows"


@pytest.mark.asyncio
async def test_dual_failure_recovery(compose_env):
    """Kill both orchestrators, restart both, verify both runs recover."""
    # 1. Upload workflows — store on whichever orch, both share SQLite
    seq_flow_id = await store_flow(
        ORCH1_URL, str(WORKFLOWS / "sequential_delay.yaml")
    )
    par_flow_id = await store_flow(
        ORCH2_URL, str(WORKFLOWS / "parallel_delay.yaml")
    )

    # 2. Submit run-A (sequential) to orch-1, run-B (parallel) to orch-2
    run_a_id = await submit_run(ORCH1_URL, seq_flow_id, {"data": {"run": "A"}})
    run_b_id = await submit_run(ORCH2_URL, par_flow_id, {"data": {"run": "B"}})

    # 3. Wait for execution to begin on both
    assert poll_tracker_for_step("step1", timeout=15), "step1 (run A) did not start"
    assert poll_tracker_for_step("parallel_a", timeout=15), "parallel_a (run B) did not start"

    # 4. Verify both flows are still in-progress before killing.
    #    step3 depends on step2 (5s+5s after step1), aggregate depends on
    #    parallel_d (7s) — neither can have completed this early.
    pre_kill_records = read_tracker_records()
    assert count_step_executions(pre_kill_records, "step3") == 0, (
        "step3 should not have executed before kill — sequential flow should still be in-progress"
    )
    assert count_step_executions(pre_kill_records, "aggregate") == 0, (
        "aggregate should not have executed before kill — parallel flow should still be in-progress"
    )

    # 5. Kill BOTH orchestrators
    docker_kill("orchestrator-1", "orchestrator-2")

    # 6. Wait for etcd leases to expire
    await asyncio.sleep(10)

    # 7. Restart both
    docker_start("orchestrator-1", "orchestrator-2")
    wait_for_health(ORCH1_URL, timeout=30)
    wait_for_health(ORCH2_URL, timeout=30)

    # 8. Wait for both runs to complete (either orchestrator may claim either run)
    result_a = await wait_for_run_on_either(run_a_id, timeout=90)
    result_b = await wait_for_run_on_either(run_b_id, timeout=90)

    # 9. Assertions — both runs succeeded
    assert result_a["status"] == "completed", f"Run A: {result_a['status']}"
    assert result_b["status"] == "completed", f"Run B: {result_b['status']}"

    records = read_tracker_records()

    # Verify recovery produced new work for both runs
    assert count_step_executions(records, "step3", run_a_id) >= 1, (
        "step3 should have been executed by recovery — sequential run did not produce new work"
    )
    assert count_step_executions(records, "aggregate", run_b_id) >= 1, (
        "aggregate should have been executed by recovery — parallel run did not produce new work"
    )

    # All steps executed at least once (at-least-once semantics).
    for label in ["step1", "step2"]:
        assert count_step_executions(records, label, run_a_id) >= 1, f"{label} should have executed"
    for label in ["parallel_a", "parallel_b", "parallel_c", "parallel_d"]:
        assert count_step_executions(records, label, run_b_id) >= 1, f"{label} should have executed"

    # Attempt tracking: step1 completed before the crash, not retried.
    step1_records = get_step_tracker_records(records, "step1", run_a_id)
    assert step1_records[0]["attempt"] == 1, "step1 should not have been retried"
    assert step1_records[0]["tracker_attempt"] == 1, "step1 should have a single execution"

    # step3 and aggregate dispatched fresh by the recovered orchestrators.
    step3_records = get_step_tracker_records(records, "step3", run_a_id)
    assert step3_records[-1]["attempt"] == 1, "step3 should be attempt 1 (fresh dispatch)"

    agg_records = get_step_tracker_records(records, "aggregate", run_b_id)
    assert agg_records[-1]["attempt"] == 1, "aggregate should be attempt 1 (fresh dispatch)"
