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

import logging
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

        @self.server.component(subpath="/classify")
        async def classify(input_data: Any, context: StepflowContext) -> Any:
            """Probe a document to determine optimal pipeline configuration."""
            return await classify_document(input_data, context)

        @self.server.component(subpath="/convert")
        async def convert(input_data: Any, context: StepflowContext) -> Any:
            """Convert a document using docling's DocumentConverter directly."""
            return await convert_document(input_data, context, self._converter_cache)

        @self.server.component(subpath="/chunk")
        async def chunk(input_data: Any, context: StepflowContext) -> Any:
            """Chunk a DoclingDocument using docling's HybridChunker."""
            return await chunk_document(input_data, context)
