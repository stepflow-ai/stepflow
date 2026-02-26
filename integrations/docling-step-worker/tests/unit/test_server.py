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

"""Tests for component registration and server behavior."""

from __future__ import annotations

from unittest.mock import patch

import pytest

from docling_step_worker.converter_cache import ConverterCache
from docling_step_worker.server import DoclingStepWorkerServer


class TestServerRegistration:
    """Tests for component registration."""

    @patch("docling_step_worker.converter_cache.DocumentConverter")
    def test_registers_three_components(self, mock_converter_cls):
        server = DoclingStepWorkerServer()
        components = server.server.get_components()
        component_names = set(components.keys())

        assert "/classify" in component_names
        assert "/convert" in component_names
        assert "/chunk" in component_names

    @patch("docling_step_worker.converter_cache.DocumentConverter")
    def test_has_converter_cache(self, mock_converter_cls):
        server = DoclingStepWorkerServer()
        assert isinstance(server._converter_cache, ConverterCache)
        assert server._converter_cache.size == 0


class TestComponentCallable:
    """Tests that components can be invoked."""

    @patch("docling_step_worker.converter_cache.DocumentConverter")
    @patch("docling_step_worker.server.classify_document")
    @pytest.mark.asyncio
    async def test_classify_component_callable(self, mock_classify, mock_converter_cls):
        mock_classify.return_value = {"recommended_config": "default", "page_count": 1}
        server = DoclingStepWorkerServer()

        result = server.server.get_component("/classify")
        assert result is not None
        component, params = result
        assert component.name == "/classify"

    @patch("docling_step_worker.converter_cache.DocumentConverter")
    @patch("docling_step_worker.server.convert_document")
    @pytest.mark.asyncio
    async def test_convert_component_callable(self, mock_convert, mock_converter_cls):
        mock_convert.return_value = {"status": "success", "document": {}}
        server = DoclingStepWorkerServer()

        result = server.server.get_component("/convert")
        assert result is not None
        component, params = result
        assert component.name == "/convert"

    @patch("docling_step_worker.converter_cache.DocumentConverter")
    @patch("docling_step_worker.server.chunk_document")
    @pytest.mark.asyncio
    async def test_chunk_component_callable(self, mock_chunk, mock_converter_cls):
        mock_chunk.return_value = {"chunks": [], "chunk_count": 0}
        server = DoclingStepWorkerServer()

        result = server.server.get_component("/chunk")
        assert result is not None
        component, params = result
        assert component.name == "/chunk"
