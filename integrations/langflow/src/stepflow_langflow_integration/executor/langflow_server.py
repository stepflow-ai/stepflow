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

Provides component handlers for executing Langflow components:
- /langflow/custom_code: Executes components by compiling code from blobs
- /langflow/core/{*component}: Executes known core components by module path
"""

import asyncio
import logging
from typing import Any

from stepflow_py.worker import StepflowContext, StepflowServer
from stepflow_py.worker.observability import setup_observability

from .core_executor import CoreExecutor
from .custom_code_executor import CustomCodeExecutor

logger = logging.getLogger(__name__)


class StepflowLangflowServer:
    """Stepflow component server with Langflow execution capabilities.

    This server provides handlers for executing Langflow components:
    - custom_code: Compiles and executes component code from blob storage
    - core/{*component}: Executes known core components by module path

    Uses a pre-compilation approach that eliminates context calls during execution.
    """

    def __init__(self):
        """Initialize the Langflow component server."""
        self.server = StepflowServer()
        self.custom_code_executor = CustomCodeExecutor()
        self.core_executor = CoreExecutor()

        # Register components
        self._register_components()

    def _register_components(self) -> None:
        """Register all Langflow components."""

        @self.server.component
        async def custom_code(
            input_data: dict[str, Any], context: StepflowContext
        ) -> dict[str, Any]:
            """Execute a Langflow custom code component.

            Compiles component code from blob storage and executes it.
            Uses pre-compilation to prevent JSON-RPC deadlocks.
            """
            return await self.custom_code_executor.execute(input_data, context)

        @self.server.component(subpath="core/{*component}")
        async def core(
            input_data: dict[str, Any],
            context: StepflowContext,
            component: str,
        ) -> dict[str, Any]:
            """Execute a known core Langflow component by module path.

            The component path is captured from the URL wildcard and converted
            to a Python module path for direct import and execution.

            Args:
                input_data: Component input with template, outputs, runtime inputs
                context: Stepflow context for runtime services
                component: From wildcard, e.g., "lfx/components/.../Class"
            """
            return await self.core_executor.execute(component, input_data, context)

        # TODO: Register native component implementations
        # self.server.component(subpath="openai_chat", func=self._openai_chat)
        # self.server.component(subpath="chat_input", func=self._chat_input)

    def run_grpc(
        self,
        tasks_url: str = "localhost:7837",
        queue_name: str = "langflow",
        max_concurrent: int = 4,
    ) -> None:
        """Run the component server as a gRPC pull-based worker."""
        import os

        import nest_asyncio  # type: ignore
        from stepflow_py.worker.grpc_worker import run_grpc_worker

        # Initialize observability (tracing, logging) before anything else
        setup_observability()

        logger.info("Langflow component server starting (gRPC)")

        # Apply nest_asyncio to allow nested event loops
        # This is needed because Langflow components may call asyncio.run()
        # from within an already-running event loop
        nest_asyncio.apply()

        # Ensure Langflow services (especially DatabaseService) are properly
        # initialized when a database URL is configured.
        if os.environ.get("LANGFLOW_DATABASE_URL"):
            from langflow.services.utils import (
                initialize_services,
                teardown_services,
            )

            asyncio.run(teardown_services())
            asyncio.run(initialize_services())

        asyncio.run(
            run_grpc_worker(
                server=self.server,
                tasks_url=tasks_url,
                queue_name=queue_name,
                max_concurrent=max_concurrent,
            )
        )
