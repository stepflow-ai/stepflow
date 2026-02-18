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
import time
from datetime import datetime, timezone

import msgspec

from stepflow_py.worker import StepflowServer

TRACKER_FILE = os.environ.get("TRACKER_FILE", "/tracker/executions.jsonl")

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


@server.component
async def delay(input: DelayInput) -> DelayOutput:
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

    # Write tracker record
    record = {
        "step_label": input.step_label,
        "executed_at": executed_at,
        "duration": duration,
        "pid": os.getpid(),
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
    )


if __name__ == "__main__":
    asyncio.run(server.run(host="0.0.0.0", port=8080))
