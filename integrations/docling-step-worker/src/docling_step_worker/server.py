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
from typing import Any

from docling.datamodel.base_models import InputFormat
from docling.document_converter import DocumentConverter, PdfFormatOption
from docling.pipeline.standard_pdf_pipeline import StandardPdfPipeline
from stepflow_py.worker import StepflowContext, StepflowServer

from docling_step_worker.chunk import chunk_document
from docling_step_worker.classify import classify_document
from docling_step_worker.config import PIPELINE_CONFIGS
from docling_step_worker.convert import convert_document

logger = logging.getLogger(__name__)


class DoclingStepWorkerServer:
    """Stepflow component server wrapping docling's DocumentConverter directly.

    Maintains one DocumentConverter per named pipeline config (from PIPELINE_CONFIGS).
    Each converter loads models once and caches its pipeline instance.
    No docling-serve sidecar needed.
    """

    def __init__(self) -> None:
        self.server = StepflowServer()
        self._converters: dict[str, DocumentConverter] = {}
        self._register_components()

    def _get_converter(self, config_name: str = "default") -> DocumentConverter:
        """Get or create a DocumentConverter for the named pipeline config.

        DocumentConverter.convert() does NOT accept pipeline_options —
        options are fixed at construction time. So we maintain one converter
        per named config. The converter's internal cache ensures models are
        loaded only once per distinct (pipeline_class, options_hash).
        """
        if config_name not in self._converters:
            if config_name not in PIPELINE_CONFIGS:
                logger.warning(
                    "Unknown config '%s', falling back to 'default'", config_name
                )
                config_name = "default"
            logger.info(
                "Initializing DocumentConverter for config '%s'...", config_name
            )
            pipeline_options = PIPELINE_CONFIGS[config_name]
            self._converters[config_name] = DocumentConverter(
                format_options={
                    InputFormat.PDF: PdfFormatOption(
                        pipeline_cls=StandardPdfPipeline,
                        pipeline_options=pipeline_options,
                    )
                }
            )
            logger.info("DocumentConverter for '%s' initialized.", config_name)
        return self._converters[config_name]

    def _register_components(self) -> None:
        """Register all docling components."""

        @self.server.component(name="/classify")
        async def classify(input_data: Any, context: StepflowContext) -> Any:
            """Probe a document to determine optimal pipeline configuration."""
            return await classify_document(input_data, context)

        @self.server.component(name="/convert")
        async def convert(input_data: Any, context: StepflowContext) -> Any:
            """Convert a document using docling's DocumentConverter directly."""
            config_name = input_data.get("pipeline_config", "default")
            converter = self._get_converter(config_name)
            return await convert_document(input_data, context, converter)

        @self.server.component(name="/chunk")
        async def chunk(input_data: Any, context: StepflowContext) -> Any:
            """Chunk a DoclingDocument using docling's HybridChunker."""
            return await chunk_document(input_data, context)

    def run(self, *args: Any, **kwargs: Any) -> None:
        """Run the component server.

        Starts the Stepflow HTTP server which handles
        JSON-RPC communication with the Stepflow runtime.
        """
        import nest_asyncio

        nest_asyncio.apply()

        logger.info("Starting Docling Step Worker server...")
        asyncio.run(self.server.run(*args, **kwargs))
