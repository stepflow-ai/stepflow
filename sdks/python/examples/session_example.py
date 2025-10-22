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
Example demonstrating session_id access and structured logging.
This shows how HTTP mode components can access their session ID
and how diagnostic context is automatically included in logs.
"""

import logging

import msgspec

from stepflow_py import StepflowContext, StepflowServer

# Create server instance
server = StepflowServer()

# Get a logger for this module - diagnostic context will be automatically included
logger = logging.getLogger(__name__)


class SessionInput(msgspec.Struct):
    message: str


class SessionOutput(msgspec.Struct):
    processed_message: str
    session_id: str | None
    transport_mode: str


@server.component
def session_aware_component(
    input: SessionInput, context: StepflowContext
) -> SessionOutput:
    """Component that demonstrates session_id access and structured logging.

    Logs from this component will automatically include:
    - flow_id: The ID of the flow being executed
    - run_id: The unique ID for this workflow execution
    - step_id: The ID of the current step
    - trace_id: OpenTelemetry trace ID (if tracing is enabled)
    - span_id: OpenTelemetry span ID (if tracing is enabled)
    """

    # Access the session ID from the context
    session_id = context.session_id

    # Determine transport mode based on session_id presence
    transport_mode = "HTTP" if session_id is not None else "STDIO"

    # Logging will automatically include diagnostic context (flow_id, run_id, step_id, trace_id, span_id)
    logger.info(f"Processing message: {input.message}")
    logger.info(f"Transport mode: {transport_mode}")

    # Access context IDs for debugging
    logger.debug(f"Flow ID: {context.flow_id}")
    logger.debug(f"Run ID: {context.run_id}")
    logger.debug(f"Step ID: {context.step_id}")
    logger.debug(f"Attempt: {context.attempt}")

    return SessionOutput(
        processed_message=f"Processed: {input.message}",
        session_id=session_id,
        transport_mode=transport_mode,
    )


if __name__ == "__main__":
    # This example can be run in both STDIO and HTTP modes
    # Logging configuration is automatically set up by the SDK
    # Configure via environment variables:
    #   STEPFLOW_LOG_LEVEL=DEBUG|INFO|WARNING|ERROR (default: INFO)
    #   STEPFLOW_LOG_DESTINATION=stderr,file,otlp (default: stderr)
    #   STEPFLOW_LOG_FILE=/path/to/file.log (if file destination)
    #   STEPFLOW_OTLP_ENDPOINT=http://localhost:4317 (for OTLP logging)

    logger.info("Session-aware component server started")
    logger.info("In STDIO mode: session_id will be None")
    logger.info("In HTTP mode: session_id will be the actual session ID")
    logger.info("All logs include diagnostic context (flow_id, run_id, step_id, etc.)")

    server.run()
