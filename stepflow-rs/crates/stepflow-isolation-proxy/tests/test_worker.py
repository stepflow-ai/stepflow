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

import msgspec

from stepflow_py.worker.server import StepflowServer
from stepflow_py.worker.vsock_worker import run_vsock_worker

logger = logging.getLogger(__name__)


class DoubleInput(msgspec.Struct):
    value: int


class DoubleOutput(msgspec.Struct):
    result: int


server = StepflowServer()


@server.component
def double(input: DoubleInput) -> DoubleOutput:
    logger.info("double called with value=%d", input.value)
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
