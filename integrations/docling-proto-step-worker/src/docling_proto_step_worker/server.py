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

"""Stepflow gRPC pull-based worker wrapping docling's DocumentConverter directly.

Uses the SDK's module-level ``server`` + ``main()`` entry point for gRPC
pull transport. Components register by name (no leading slash) — the gRPC
worker strips the leading "/" from orchestrator paths at lookup time.
"""

from __future__ import annotations

import logging
import os
from typing import Any

from stepflow_py.worker.context import StepflowContext
from stepflow_py.worker.main import main as sdk_main
from stepflow_py.worker.main import server

from docling_proto_step_worker.chunk import chunk_document
from docling_proto_step_worker.classify import classify_document
from docling_proto_step_worker.convert import convert_document
from docling_proto_step_worker.converter_cache import ConverterCache

logger = logging.getLogger(__name__)

converter_cache = ConverterCache()


@server.component(name="classify")
async def classify(input_data: Any, context: StepflowContext) -> Any:
    """Probe a document to determine optimal pipeline configuration."""
    return await classify_document(input_data, context)


@server.component(name="convert")
async def convert(input_data: Any, context: StepflowContext) -> Any:
    """Convert a document using docling's DocumentConverter directly."""
    return await convert_document(input_data, context, converter_cache)


@server.component(name="chunk")
async def chunk(input_data: Any, context: StepflowContext) -> Any:
    """Chunk a DoclingDocument using docling's HybridChunker."""
    return await chunk_document(input_data, context)


def _configure_blob_store() -> None:
    """Configure the blob store contextvars via HTTP for the current context.

    The gRPC worker's ``GrpcContext`` has its own blob ops, but the domain
    modules (``blob_utils.py``) call the module-level ``blob_store`` API
    which is backed by HTTP contextvars. We configure those here before
    ``asyncio.run()`` so all tasks inherit the configuration.

    Falls back to setting ``server._blob_api_url`` which the SDK reads
    during per-task setup.
    """
    blob_url = os.environ.get("STEPFLOW_BLOB_API_URL")
    if not blob_url:
        logger.warning(
            "STEPFLOW_BLOB_API_URL is not set. The blob store is required "
            "for resolving $blob references from the orchestrator. Set this "
            "environment variable to the orchestrator's blob API endpoint "
            "(e.g. http://orchestrator:7840/api/v1/blobs)."
        )
        return

    # Set the URL on the StepflowServer instance. The SDK's grpc_worker
    # reads server._blob_api_url and passes it to the GrpcContext, plus
    # configures blob_store contextvars within each task's async scope.
    server._blob_api_url = blob_url
    logger.info("Blob store URL configured on server instance: %s", blob_url)


def entry_point() -> None:
    """CLI entry point for the gRPC pull-based docling worker."""
    from stepflow_py.worker.observability import setup_observability

    setup_observability()
    _configure_blob_store()
    sdk_main()


if __name__ == "__main__":
    entry_point()
