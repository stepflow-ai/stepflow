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

"""Stepflow component server for Langflow integration.

Clean architecture without CachedStepflowContext.
"""

from typing import Any

from stepflow_py import StepflowContext, StepflowServer

from .udf_executor import UDFExecutor


class StepflowLangflowServer:
    """Stepflow component server with Langflow UDF execution capabilities.

    This server uses the new pre-compilation approach that eliminates the need
    for CachedStepflowContext by pre-compiling all components before execution.
    """

    def __init__(self):
        """Initialize the Langflow component server."""
        self.server = StepflowServer()
        self.udf_executor = UDFExecutor()

        # Register components
        self._register_components()

    def _register_components(self) -> None:
        """Register all Langflow components."""

        @self.server.component(name="udf_executor")
        async def udf_executor(
            input_data: dict[str, Any], context: StepflowContext
        ) -> dict[str, Any]:
            """Execute a Langflow UDF component using the new pre-compilation approach.

            This method uses the new UDF executor that pre-compiles all components
            from blob data before execution, eliminating the need for context calls
            during component execution and preventing JSON-RPC deadlocks.
            """
            # Use the new UDF executor directly - it handles pre-compilation internally
            return await self.udf_executor.execute(input_data, context)

        @self.server.component(name="mode_check")
        async def mode_check(input_data: dict[str, Any]) -> dict[str, Any]:
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
            if mode == "ingest":
                return {"skip_ingest_step": False, "skip_retrieve_step": True}
            elif mode == "retrieve":
                return {"skip_ingest_step": True, "skip_retrieve_step": False}
            else:  # hybrid or unknown - run both (skip neither)
                return {"skip_ingest_step": False, "skip_retrieve_step": False}

        @self.server.component(name="mode_output")
        async def mode_output(input_data: dict[str, Any]) -> dict[str, Any]:
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
                return {
                    "message": "Workflow completed but no retrieval result available."
                }

        # TODO: Register native component implementations
        # self.server.component(name="openai_chat", func=self._openai_chat)
        # self.server.component(name="chat_input", func=self._chat_input)

    def run(self) -> None:
        """Run the component server in STDIO mode."""
        self.server.start_stdio()

    async def serve(self, host: str = "localhost", port: int = 8000) -> None:
        """Run the component server in HTTP mode.

        Args:
            host: Server host
            port: Server port
        """
        await self.server.start_http(host=host, port=port)


if __name__ == "__main__":
    """Run the Langflow component server."""
    server = StepflowLangflowServer()
    server.run()
