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

"""Worker crash scenarios.

These tests verify the orchestrator's retry behavior when the worker
process crashes during component execution. The orchestrator retries
with configurable fibonacci backoff while the worker restarts.

Note: Delay control is used to synchronise steps *before* crashes.
After a crash destroys the in-memory delay registry, the tests simply
wait for container health and then poll the run status to completion.
"""

from __future__ import annotations

import asyncio
import time
from pathlib import Path

import pytest

from helpers import (
    ORCH1_URL,
    count_step_executions,
    crash_worker,
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
    wait_for_worker_health,
)

WORKFLOWS = Path(__file__).parent / "workflows"


@pytest.mark.asyncio
async def test_worker_crash_single_retry(compose_env):
    """Scenario D: Worker crashes once, orchestrator retries and succeeds.

    crash_worker() sends SIGUSR1 to the worker's PID 1, causing os._exit(1).
    Docker's ``restart: unless-stopped`` policy auto-restarts the container.

    With gRPC pull transport, the orchestrator detects the crash via heartbeat
    timeout (~5s), then retries. The restarted worker reconnects via gRPC and
    picks up the retried task.
    """
    # 1. Submit a sequential run (step1=5s, step2=5s, step3=5s)
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "sequential_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "worker_crash"}})

    # 2. Wait for step1 to be held, release it so it completes quickly
    poll_for_delay("step1", run_id=run_id, timeout=15)
    release_delay("step1", run_id=run_id)

    # 3. Wait for step2 to be held — step1 is now complete
    poll_for_delay("step2", run_id=run_id, timeout=15)

    # 4. Crash the worker while step2 is held — this destroys the delay
    #    registry and interrupts step2. Docker auto-restarts the container.
    crash_worker()

    # 5. Wait for the auto-restarted worker to become healthy.
    wait_for_worker_health(timeout=30)

    # 6. Orchestrator retries step2. With pull transport, detection takes ~5s
    #    (heartbeat timeout) + fibonacci backoff + worker reconnect time.
    poll_for_delay("step2", run_id=run_id, timeout=45)
    release_delay("step2", run_id=run_id)

    poll_for_delay("step3", run_id=run_id, timeout=45)
    release_delay("step3", run_id=run_id)

    # 7. Wait for run to complete
    result = await wait_for_run(ORCH1_URL, run_id, timeout=120)
    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    # 7. Verify attempt tracking
    records = read_tracker_records()

    # step1 completed before the crash — should have exactly 1 execution.
    assert count_step_executions(records, "step1", run_id) == 1, (
        "step1 should not be re-executed — it completed before the crash"
    )
    step1_records = get_step_tracker_records(records, "step1", run_id)
    assert step1_records[0]["attempt"] == 1, "step1 should be attempt 1 (completed before crash)"

    # step2 was interrupted by the crash. The orchestrator should have retried.
    # The successful execution should show attempt >= 2.
    step2_records = get_step_tracker_records(records, "step2", run_id)
    assert len(step2_records) >= 1, "step2 should have at least one tracker record"
    successful_record = step2_records[-1]
    assert successful_record["attempt"] >= 2, (
        f"step2 should show attempt >= 2 after worker crash retry, got {successful_record['attempt']}"
    )

    # All three steps should have completed
    for label in ["step1", "step2", "step3"]:
        assert count_step_executions(records, label, run_id) >= 1, (
            f"{label} should have executed"
        )


@pytest.mark.asyncio
async def test_worker_crash_exhausts_retries(compose_env):
    """Scenario E: Worker stays down, orchestrator exhausts retries.

    Kill the worker and leave it dead. With pull transport, each retry
    puts the task in the queue where it times out (queueTimeoutSecs=10).
    After transportMaxRetries=3 retries, the run fails.
    """
    # 1. Submit a sequential run
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "sequential_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "exhaust_retries"}})

    # 2. Wait for step1 to be held (confirms the step is executing)
    poll_for_delay("step1", run_id=run_id, timeout=15)

    # 3. Kill the worker and leave it dead. We use docker_kill (not
    #    crash_worker) because docker_kill is a manual stop that prevents
    #    Docker's restart policy from bringing it back.
    #    With max_attempts=3 and fibonacci backoff (2s, 2s), retries exhaust
    #    in ~5s.
    docker_kill("worker")

    # 4. The run should have failed (all retries exhausted).
    #    With pull transport: heartbeat timeout (5s) + 3 retries * queue timeout (10s) ≈ 35s
    result = await wait_for_run(ORCH1_URL, run_id, timeout=60)
    assert result["status"] == "failed", f"Expected failed, got {result['status']}"

    # 5. Restart worker for cleanup (next test starts fresh via compose_down,
    #    but restart here in case of test ordering changes)
    docker_start("worker")


@pytest.mark.asyncio
async def test_worker_and_orchestrator_crash(compose_env):
    """Scenario F: Both worker and orchestrator crash, full recovery.

    After both restart, the orchestrator recovers from journal and
    re-dispatches incomplete steps to the now-healthy worker.
    """
    # 1. Submit a sequential run and wait for step1 to be held, then release
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "sequential_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "dual_crash"}})

    poll_for_delay("step1", run_id=run_id, timeout=15)
    release_delay("step1", run_id=run_id)

    # 2. Wait for step2 to be held — step1 is now durably complete
    poll_for_delay("step2", run_id=run_id, timeout=15)

    # 3. Kill orchestrator first (step2 is deterministically held)
    docker_kill("orchestrator-1")

    # 4. Kill worker (destroys delay registry and any in-progress work)
    docker_kill("worker")

    # 5. Restart worker first (so it's ready for requests)
    docker_start("worker")
    wait_for_worker_health(timeout=30)

    # 6. Restart orchestrator — it should recover from journal
    docker_start("orchestrator-1")
    wait_for_health(ORCH1_URL, timeout=30)

    # 7. Recovery re-dispatches step2. With pull transport, the worker needs
    #    time to reconnect to the restarted orchestrator via gRPC.
    poll_for_delay("step2", run_id=run_id, timeout=45)
    release_delay("step2", run_id=run_id)

    poll_for_delay("step3", run_id=run_id, timeout=45)
    release_delay("step3", run_id=run_id)

    # 8. Wait for the run to complete
    result = await wait_for_run(ORCH1_URL, run_id, timeout=90)
    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    # 8. Verify step1 was not re-executed (journaled before crash)
    records = read_tracker_records()
    assert count_step_executions(records, "step1", run_id) == 1, (
        "step1 should not be re-executed — it was journaled before the crash"
    )

    # All steps completed
    for label in ["step1", "step2", "step3"]:
        assert count_step_executions(records, label, run_id) >= 1, (
            f"{label} should have executed"
        )

    # step3 dispatched fresh by the recovered orchestrator.
    # It was never started before the crash, so attempt=1 is correct.
    step3_records = get_step_tracker_records(records, "step3", run_id)
    assert step3_records[-1]["attempt"] == 1, (
        "step3 should be attempt 1 (fresh dispatch after recovery)"
    )


@pytest.mark.asyncio
async def test_orchestrator_crash_worker_delivers_result(compose_env):
    """Scenario G: Orchestrator crashes while worker has in-flight task.

    The worker completes the task, but CompleteTask fails because the
    orchestrator is down. The worker retries CompleteTask with exponential
    backoff. When the orchestrator recovers, it re-registers the same
    task_id (from the journal), and the worker's retry delivers the result
    without re-executing the step.

    This tests the full task_id journalling + CompleteTask retry path:
    1. Task_id is journalled in TasksStarted before dispatch
    2. On recovery, the same task_id is re-registered in TaskRegistry
    3. Worker's CompleteTask retry with the original task_id lands
    4. Step is NOT re-executed
    """
    # 1. Submit a sequential run and wait for step1 to be held, then release
    flow_id = await store_flow(ORCH1_URL, str(WORKFLOWS / "sequential_delay.yaml"))
    run_id = await submit_run(ORCH1_URL, flow_id, {"data": {"value": "orch_crash_result"}})

    poll_for_delay("step1", run_id=run_id, timeout=15)
    release_delay("step1", run_id=run_id)

    # 2. Wait for step2 to be held — step1 is now durably complete
    poll_for_delay("step2", run_id=run_id, timeout=15)

    # 3. Kill orchestrator while step2 is in-flight on the worker
    docker_kill("orchestrator-1")

    # 4. Release step2 — the worker completes execution but CompleteTask
    #    fails (orchestrator down). The worker starts retrying.
    release_delay("step2", run_id=run_id)

    # 5. Give the worker a moment to attempt CompleteTask and start retrying
    await asyncio.sleep(3)

    # 6. Restart orchestrator — it recovers from journal and re-registers
    #    the same task_id for step2 in the TaskRegistry.
    docker_start("orchestrator-1")
    wait_for_health(ORCH1_URL, timeout=30)

    # 7. The worker's CompleteTask retry should land, delivering step2's
    #    result without re-execution. Then step3 gets dispatched.
    poll_for_delay("step3", run_id=run_id, timeout=60)
    release_delay("step3", run_id=run_id)

    # 8. Wait for the run to complete
    result = await wait_for_run(ORCH1_URL, run_id, timeout=90)
    assert result["status"] == "completed", f"Expected completed, got {result['status']}"

    # 9. Key assertion: step2 should have exactly 1 tracker record.
    #    If the worker's CompleteTask retry landed successfully, the
    #    orchestrator accepted the result and did NOT re-dispatch step2.
    records = read_tracker_records()
    step2_count = count_step_executions(records, "step2", run_id)
    assert step2_count == 1, (
        f"step2 should have been executed exactly once (worker delivered result "
        f"via CompleteTask retry), but was executed {step2_count} times"
    )

    # step1 completed before crash, step3 dispatched after recovery
    assert count_step_executions(records, "step1", run_id) == 1
    assert count_step_executions(records, "step3", run_id) >= 1
