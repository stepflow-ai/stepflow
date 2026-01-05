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

import logging
from typing import Any

from stepflow_server import StepflowContext, StepflowServer

from .udf_executor import UDFExecutor

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    force=True,  # Override any existing configuration
)


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

        # TODO: Register native component implementations
        # self.server.component(name="openai_chat", func=self._openai_chat)
        # self.server.component(name="chat_input", func=self._chat_input)

    def run(self) -> None:
        """Run the component server in STDIO mode."""
        self.server.start_stdio()

    async def serve(
        self,
        host: str = "localhost",
        port: int = 8000,
        workers: int = 3,
        backlog: int = 128,
        timeout_keep_alive: int = 5,
    ) -> None:
        """Run the component server in HTTP mode.

        Args:
            host: Server host
            port: Server port
            workers: Number of worker processes
            backlog: Maximum number of pending connections
            timeout_keep_alive: Keep-alive timeout in seconds
        """
        # Apply nest_asyncio to allow nested event loops in HTTP mode
        # This is needed because Langflow components may call asyncio.run()
        # from within an already-running event loop
        import nest_asyncio  # type: ignore

        nest_asyncio.apply()

        await self.server.start_http(
            host=host,
            port=port,
            workers=workers,
            backlog=backlog,
            timeout_keep_alive=timeout_keep_alive,
        )


if __name__ == "__main__":
    """Run the Langflow component server."""
    server = StepflowLangflowServer()
    server.run()
