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

"""Stepflow component server wrapping docling's DocumentConverter directly.

No docling-serve sidecar. No HTTP proxy. The docling library runs
in the same process as the Stepflow worker.
"""

from __future__ import annotations

import asyncio
import logging
import os
from typing import Any

from stepflow_py.worker import StepflowContext, StepflowServer

from docling_step_worker.chunk import chunk_document
from docling_step_worker.classify import classify_document
from docling_step_worker.convert import convert_document
from docling_step_worker.converter_cache import ConverterCache

logger = logging.getLogger(__name__)


class DoclingStepWorkerServer:
    """Stepflow component server wrapping docling's DocumentConverter directly.

    Uses a ConverterCache that supports both named pipeline configs (from
    classify step) and per-request options (docling-serve compatible).
    No docling-serve sidecar needed.
    """

    def __init__(self) -> None:
        self.server = StepflowServer()
        self._converter_cache = ConverterCache()
        self._register_components()

    def _register_components(self) -> None:
        """Register all docling components."""

        @self.server.component(name="/classify")
        async def classify(input_data: Any, context: StepflowContext) -> Any:
            """Probe a document to determine optimal pipeline configuration."""
            return await classify_document(input_data, context)

        @self.server.component(name="/convert")
        async def convert(input_data: Any, context: StepflowContext) -> Any:
            """Convert a document using docling's DocumentConverter directly."""
            return await convert_document(input_data, context, self._converter_cache)

        @self.server.component(name="/chunk")
        async def chunk(input_data: Any, context: StepflowContext) -> Any:
            """Chunk a DoclingDocument using docling's HybridChunker."""
            return await chunk_document(input_data, context)

    def _configure_blob_store(self) -> None:
        """Configure the blob store URL on the SDK server instance.

        In K8s deployments the orchestrator sends the blob API URL during
        initialization, but the load balancer may proxy the session so the
        worker never receives it.  ``STEPFLOW_BLOB_API_URL`` provides an
        explicit override that is set directly on the ``StepflowServer``
        instance.  The SDK's per-request handler reads
        ``server.blob_api_url`` and configures the contextvar-based blob
        store within each request's async scope — so this survives
        ``asyncio.run()`` boundaries without issues.

        Raises:
            SystemExit: If ``STEPFLOW_BLOB_API_URL`` is not set.  The blob
                store is essential for resolving ``$blob`` references that
                the orchestrator creates for large inputs.
        """
        blob_url = os.environ.get("STEPFLOW_BLOB_API_URL")
        if not blob_url:
            logger.error(
                "STEPFLOW_BLOB_API_URL is not set. The blob store is required "
                "for resolving $blob references from the orchestrator. Set this "
                "environment variable to the orchestrator's blob API endpoint "
                "(e.g. http://orchestrator:7840/api/v1/blobs)."
            )
            raise SystemExit(1)

        # Set the URL directly on the StepflowServer instance.  The SDK's
        # http_server.handle_request() reads self.server.blob_api_url on
        # every request and calls blob_store.configure() within the request
        # async context, which correctly scopes the ContextVar.
        self.server._blob_api_url = blob_url
        logger.info("Blob store URL configured on server instance: %s", blob_url)

    def run(self, *args: Any, **kwargs: Any) -> None:
        """Run the component server.

        Starts the Stepflow HTTP server which handles
        JSON-RPC communication with the Stepflow runtime.
        """
        import nest_asyncio

        nest_asyncio.apply()

        self._configure_blob_store()

        logger.info("Starting Docling Step Worker server...")
        asyncio.run(self.server.run(*args, **kwargs))
