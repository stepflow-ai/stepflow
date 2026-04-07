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

"""Minimal Python vsock worker for integration testing.

Registers a simple 'double' component and runs the vsock worker in oneshot mode.

Usage:
    python test_worker.py --socket /tmp/test.sock --oneshot
"""

import asyncio
import logging
import sys

# Set up logging to stderr immediately
logging.basicConfig(
    level=logging.DEBUG,
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    stream=sys.stderr,
)

import time as _time

import msgspec

from stepflow_py.worker.server import StepflowServer
from stepflow_py.worker.vsock_worker import run_vsock_worker

logger = logging.getLogger(__name__)

# Simulate a heavy dependency load (like Langflow, ML models, etc.)
# In a subprocess worker, this cost is paid on EVERY task invocation.
# In Firecracker with snapshots, this is paid once at snapshot creation time.
import hashlib

_heavy_load_start = _time.monotonic()
_lookup_table = {}
for _i in range(500_000):
    _key = hashlib.md5(str(_i).encode()).hexdigest()[:12]
    _lookup_table[_key] = _i * 42
_heavy_load_ms = (_time.monotonic() - _heavy_load_start) * 1000
logger.info(
    "TIMING: heavy_dependency_load=%.0fms (%d entries)", _heavy_load_ms, len(_lookup_table)
)


class DoubleInput(msgspec.Struct):
    value: int


class DoubleOutput(msgspec.Struct):
    result: int


server = StepflowServer()


@server.component
def double(input: DoubleInput) -> DoubleOutput:
    # Use the pre-loaded lookup table to prove data is accessible
    key = hashlib.md5(str(input.value).encode()).hexdigest()[:12]
    looked_up = _lookup_table.get(key)
    expected = input.value * 42
    logger.info(
        "double(%d): lookup_table has %d entries, verified=%s",
        input.value,
        len(_lookup_table),
        looked_up == expected,
    )
    return DoubleOutput(result=input.value * 2)


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("--socket", required=True)
    parser.add_argument("--oneshot", action="store_true")
    args = parser.parse_args()

    logger.info("Starting test worker on socket=%s oneshot=%s", args.socket, args.oneshot)

    asyncio.run(
        run_vsock_worker(
            server=server,
            socket_path=args.socket,
            oneshot=args.oneshot,
        )
    )
