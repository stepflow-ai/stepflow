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

import sys
from pathlib import Path
from typing import Any

# Add the package root to the path before importing project modules
package_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(package_root))

from stepflow_py import StepflowContext, StepflowStdioServer

from stepflow_langflow_integration.components.component_tool import (
    component_tool_executor,
)
from stepflow_langflow_integration.executor.udf_executor import UDFExecutor

# Create server instance (following the exact pattern from stepflow_py/main.py)
server = StepflowStdioServer()

# Create UDF executor
udf_executor = UDFExecutor()


# Register the main UDF executor component at module level
@server.component(name="udf_executor")
async def udf_executor_component(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Execute a Langflow UDF component."""
    return await udf_executor.execute(input_data, context)


# Register the component tool wrapper component
@server.component(name="component_tool")
async def component_tool_component(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Create tool wrappers from Langflow components."""
    return await component_tool_executor(input_data, context)


# Register the mode check component for vector store workflows
@server.component(name="mode_check")
async def mode_check_component(input_data: dict[str, Any]) -> dict[str, Any]:
    """Check vector store execution mode and return skip flags.

    Args:
        input_data: Dict with "mode" key ("ingest", "retrieve", or "hybrid")

    Returns:
        Dict with boolean skip flags (True means skip the step):
        - skip_ingest_step: True if ingestion steps should be skipped
        - skip_retrieve_step: True if retrieval steps should be skipped
    """
    mode = input_data.get("mode", "hybrid")

    # Determine which steps should be skipped based on mode
    # skipIf expects True to skip the step
    if mode == "ingest":
        # In ingest mode: run ingest, skip retrieve
        return {"skip_ingest_step": False, "skip_retrieve_step": True}
    elif mode == "retrieve":
        # In retrieve mode: skip ingest, run retrieve
        return {"skip_ingest_step": True, "skip_retrieve_step": False}
    else:  # hybrid or unknown - run both (skip neither)
        return {"skip_ingest_step": False, "skip_retrieve_step": False}


# Register the mode output component for vector store workflows
@server.component(name="mode_output")
async def mode_output_component(input_data: dict[str, Any]) -> dict[str, Any]:
    """Generate appropriate output message based on mode and retrieval result.

    Args:
        input_data: Dict with:
            - mode: Execution mode ("ingest", "retrieve", or "hybrid")
            - retrieval_result: Optional result from retrieval path
              (may be None if skipped)

    Returns:
        Dict with message field containing appropriate output
    """
    mode = input_data.get("mode", "hybrid")
    retrieval_result = input_data.get("retrieval_result")

    if mode == "ingest":
        # Ingest mode: retrieval was skipped
        return {
            "message": "Document ingestion completed successfully. "
            "Retrieval was skipped in ingest-only mode."
        }
    elif retrieval_result is not None:
        # Retrieval happened (either retrieve or hybrid mode)
        return {"message": retrieval_result}
    else:
        # Hybrid mode but no retrieval result (shouldn't normally happen)
        return {"message": "Workflow completed but no retrieval result available."}


# All Langflow components now route through the UDF executor for real execution
# No hardcoded component implementations - everything uses real Langflow code


def main():
    """Main entry point for the Langflow component server.

    Logging is automatically configured by the SDK via setup_observability().
    Configure via environment variables:
    - STEPFLOW_LOG_LEVEL: Log level (DEBUG, INFO, WARNING, ERROR, default: INFO)
    - STEPFLOW_LOG_DESTINATION: Log destination (stderr, file, otlp)
    - STEPFLOW_OTLP_ENDPOINT: OTLP endpoint for tracing/logging
    - STEPFLOW_SERVICE_NAME: Service name (default: stepflow-python)
    """
    import nest_asyncio  # type: ignore

    nest_asyncio.apply()
    # Start the server - this handles all the asyncio setup correctly
    # and calls setup_observability() to configure logging
    server.run()


if __name__ == "__main__":
    """Run the Langflow component server."""
    main()
