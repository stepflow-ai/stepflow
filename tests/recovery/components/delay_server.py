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

"""Delay component for recovery integration tests.

Each invocation sleeps for a configurable duration, writes a tracker record
to a JSONL file, and returns the input payload along with execution metadata.
"""

import asyncio
import json
import os
import signal
import time
from datetime import datetime, timezone

import msgspec

from stepflow_py.worker import StepflowContext, StepflowServer

TRACKER_FILE = os.environ.get("TRACKER_FILE", "/tracker/executions.jsonl")

# SIGUSR1 handler for test-triggered crashes. Calling os._exit() causes the
# container's PID 1 to exit, which Docker's restart policy will auto-restart.
signal.signal(signal.SIGUSR1, lambda *_: os._exit(1))

server = StepflowServer()


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

    # Sleep in small increments to allow clean shutdown
    remaining = input.seconds
    while remaining > 0:
        await asyncio.sleep(min(0.5, remaining))
        remaining -= 0.5

    duration = time.monotonic() - start
    executed_at = datetime.now(timezone.utc).isoformat()

    if input.should_fail:
        raise RuntimeError(f"Intentional failure for step {input.step_label}")

    run_id = context.run_id or ""
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


if __name__ == "__main__":
    asyncio.run(server.run(host="0.0.0.0", port=8080))
