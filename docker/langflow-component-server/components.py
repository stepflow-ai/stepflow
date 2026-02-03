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
Langflow Component Server entry point for Kubernetes deployments.

This server provides HTTP-based component execution for Stepflow workflows
using the Langflow integration.
"""

import argparse
import logging
import os
import uuid

# Import from installed package (installed via uv pip install)
from stepflow_langflow_integration.executor.langflow_server import StepflowLangflowServer

# Get logger - configuration is handled by SDK's setup_observability()
logger = logging.getLogger(__name__)

# Generate instance ID for load balancer routing
POD_NAME = os.getenv("POD_NAME", "local")
STARTUP_UUID = uuid.uuid4().hex[:8]
INSTANCE_ID = f"{POD_NAME}-{STARTUP_UUID}"

logger.info(f"Component server starting with instance ID: {INSTANCE_ID}")


def main():
    """Main entry point for HTTP server mode."""
    parser = argparse.ArgumentParser(description="Langflow Component Server")
    parser.add_argument(
        "--http",
        action="store_true",
        help="Run in HTTP mode (default: STDIO mode)",
    )
    parser.add_argument(
        "--host",
        type=str,
        default=os.environ.get("SERVER_HOST", "0.0.0.0"),
        help="Server host (default: 0.0.0.0)",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=int(os.environ.get("SERVER_PORT", "8080")),
        help="Server port (default: 8080)",
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=int(os.environ.get("SERVER_WORKERS", "3")),
        help="Number of worker processes (default: 3)",
    )
    parser.add_argument(
        "--backlog",
        type=int,
        default=int(os.environ.get("SERVER_BACKLOG", "128")),
        help="Maximum pending connections (default: 128)",
    )
    parser.add_argument(
        "--timeout-keep-alive",
        type=int,
        default=int(os.environ.get("SERVER_TIMEOUT_KEEP_ALIVE", "5")),
        help="Keep-alive timeout in seconds (default: 5)",
    )

    args = parser.parse_args()

    server = StepflowLangflowServer()

    if args.http:
        # Run HTTP server
        server.run(
            host=args.host,
            port=args.port,
            workers=args.workers,
            backlog=args.backlog,
            timeout_keep_alive=args.timeout_keep_alive,
        )
    else:
        # Run STDIO server (same as HTTP but with defaults)
        server.run()


if __name__ == "__main__":
    main()
