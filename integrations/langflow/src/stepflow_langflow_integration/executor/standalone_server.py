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

"""Standalone Stepflow component server for Langflow integration.

This script can be run directly and handles imports properly.
"""

import logging
import sys
from pathlib import Path
from typing import Any

# Add the package root to the path before importing project modules
package_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(package_root))

logger = logging.getLogger(__name__)

READY_SENTINEL = Path("/tmp/worker-ready")

from stepflow_py.worker import StepflowContext, StepflowServer

from stepflow_langflow_integration.components.component_tool import (
    component_tool_executor,
)
from stepflow_langflow_integration.executor.core_executor import CoreExecutor
from stepflow_langflow_integration.executor.custom_code_executor import (
    CustomCodeExecutor,
)

# Create server instance (following the exact pattern from stepflow_py.worker/main.py)
server = StepflowServer()

# Create executors
custom_code_executor = CustomCodeExecutor()
core_executor = CoreExecutor()


# Register the main custom code executor component at module level.
# Component ID = "custom_code_component" (from function name), subpath = "/custom_code".
@server.component(subpath="custom_code")
async def custom_code_component(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Execute a Langflow custom code component."""
    return await custom_code_executor.execute(input_data, context)


# Register the core component handler with wildcard subpath.
# Component ID = "core_component" (from function name), subpath = "/core/{*component}".
@server.component(subpath="core/{*component}")
async def core_component(
    input_data: dict[str, Any],
    context: StepflowContext,
    component: str,
) -> dict[str, Any]:
    """Execute a known core Langflow component by module path."""
    return await core_executor.execute(component, input_data, context)


# Register the component tool wrapper component.
# ID = "component_tool_component", subpath = "/component_tool".
@server.component(subpath="component_tool")
async def component_tool_component(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Create tool wrappers from Langflow components."""
    return await component_tool_executor(input_data, context)


# All Langflow components now route through the custom code executor for real execution
# No hardcoded component implementations - everything uses real Langflow code


def main():
    """Main entry point for the Langflow component server.

    Configure via environment variables:
    - STEPFLOW_TASKS_URL: TasksService gRPC address (default: localhost:7837)
    - STEPFLOW_QUEUE_NAME: Queue name for gRPC transport (default: langflow)
    - STEPFLOW_MAX_CONCURRENT: Max concurrent tasks (default: 4)
    - STEPFLOW_LOG_LEVEL: Log level (DEBUG, INFO, WARNING, ERROR, default: INFO)
    - STEPFLOW_LOG_DESTINATION: Log destination (stderr, file, otlp)
    - STEPFLOW_OTLP_ENDPOINT: OTLP endpoint for tracing/logging
    - STEPFLOW_SERVICE_NAME: Service name (default: stepflow-python)
    """
    import argparse
    import asyncio
    import os

    import nest_asyncio  # type: ignore

    # Initialize observability (tracing, logging) before starting server
    from stepflow_py.worker.observability import setup_observability

    setup_observability()

    nest_asyncio.apply()

    # Ensure Langflow services (especially DatabaseService) are properly
    # initialized when a database URL is configured. Without this, the lfx
    # service manager may not register langflow's DatabaseServiceFactory due
    # to platform-dependent import ordering, causing memory/message queries
    # to silently fall back to NoopDatabaseService.
    if os.environ.get("LANGFLOW_DATABASE_URL"):
        from langflow.services.utils import initialize_services, teardown_services

        asyncio.run(teardown_services())
        asyncio.run(initialize_services())

    parser = argparse.ArgumentParser(description="Langflow Stepflow Component Server")
    parser.add_argument(
        "--tasks-url",
        type=str,
        default=os.environ.get("STEPFLOW_TASKS_URL", "localhost:7837"),
        help="TasksService gRPC address (env: STEPFLOW_TASKS_URL)",
    )
    parser.add_argument(
        "--queue-name",
        type=str,
        default=os.environ.get("STEPFLOW_QUEUE_NAME", "langflow"),
        help="Queue name for gRPC transport (env: STEPFLOW_QUEUE_NAME)",
    )
    parser.add_argument(
        "--max-concurrent",
        type=int,
        default=int(os.environ.get("STEPFLOW_MAX_CONCURRENT", "4")),
        help="Max concurrent tasks (env: STEPFLOW_MAX_CONCURRENT)",
    )
    args = parser.parse_args()

    # Write ready sentinel for K8s probes before entering the pull loop
    READY_SENTINEL.touch()
    logger.info("Ready sentinel written to %s", READY_SENTINEL)

    from stepflow_py.worker.grpc_worker import run_grpc_worker

    asyncio.run(
        run_grpc_worker(
            server=server,
            tasks_url=args.tasks_url,
            queue_name=args.queue_name,
            max_concurrent=args.max_concurrent,
        )
    )


if __name__ == "__main__":
    """Run the Langflow component server."""
    main()
