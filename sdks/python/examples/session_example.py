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
Example demonstrating session_id access in components.
This shows how HTTP mode components can access their session ID.
"""

import msgspec

from stepflow_py import StepflowContext, StepflowServer

# Create server instance
server = StepflowServer()


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
    """Component that demonstrates session_id access."""

    # Access the session ID from the context
    session_id = context.session_id

    # Determine transport mode based on session_id presence
    transport_mode = "HTTP" if session_id is not None else "STDIO"

    return SessionOutput(
        processed_message=f"Processed: {input.message}",
        session_id=session_id,
        transport_mode=transport_mode,
    )


if __name__ == "__main__":
    # This example can be run in both STDIO and HTTP modes
    print("Session-aware component server started")
    print("In STDIO mode: session_id will be None")
    print("In HTTP mode: session_id will be the actual session ID")
    server.run()
