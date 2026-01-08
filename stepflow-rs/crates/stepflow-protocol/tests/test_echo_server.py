#!/usr/bin/env python3
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

"""
Simple echo server for testing HTTP protocol
"""
import argparse
import asyncio
import sys

try:
    from stepflow_py import StepflowServer
    import msgspec
except ImportError:
    print("Error: This test requires the Python SDK", file=sys.stderr)
    print("Please install with: pip install stepflow-py", file=sys.stderr)
    sys.exit(1)


class EchoInput(msgspec.Struct):
    message: str


class EchoOutput(msgspec.Struct):
    echo: str


def echo_component(input: EchoInput) -> EchoOutput:
    """Simple echo component that returns the input message"""
    return EchoOutput(echo=f"Echo: {input.message}")


async def main():
    parser = argparse.ArgumentParser(description="Echo Test Server")
    parser.add_argument("--port", type=int, default=8080, help="HTTP port")
    parser.add_argument("--host", type=str, default="127.0.0.1", help="HTTP host")

    args = parser.parse_args()

    # Create server instance
    server = StepflowServer()

    # Register the echo component
    server.component(
        echo_component, name="echo", description="Echo component for testing"
    )

    # Start HTTP server
    print(f"Starting HTTP echo server on {args.host}:{args.port}")
    await server.run(host=args.host, port=args.port)


if __name__ == "__main__":
    asyncio.run(main())
