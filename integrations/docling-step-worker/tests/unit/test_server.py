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

import logging
from unittest.mock import MagicMock, patch

import pytest

from docling_step_worker.server import DoclingStepWorkerServer


class TestServerRegistration:
    """Tests for component registration."""

    @patch("docling_step_worker.server.DocumentConverter")
    def test_registers_three_components(self, mock_converter_cls):
        server = DoclingStepWorkerServer()
        components = server.server.get_components()
        component_names = set(components.keys())

        assert "/classify" in component_names
        assert "/convert" in component_names
        assert "/chunk" in component_names

    @patch("docling_step_worker.server.DocumentConverter")
    def test_converters_empty_before_use(self, mock_converter_cls):
        server = DoclingStepWorkerServer()
        assert len(server._converters) == 0


class TestConverterManagement:
    """Tests for lazy converter initialization."""

    @patch("docling_step_worker.server.DocumentConverter")
    def test_lazy_initializes_converter_per_config(self, mock_converter_cls):
        # Return a new MagicMock for each DocumentConverter() call
        mock_converter_cls.side_effect = lambda **kwargs: MagicMock()

        server = DoclingStepWorkerServer()

        # No converters yet
        assert len(server._converters) == 0

        # First call creates converter
        converter1 = server._get_converter("default")
        assert len(server._converters) == 1
        assert "default" in server._converters

        # Second call reuses same instance
        converter2 = server._get_converter("default")
        assert converter1 is converter2
        assert len(server._converters) == 1

        # Different config creates new instance
        converter3 = server._get_converter("scanned")
        assert len(server._converters) == 2
        assert converter3 is not converter1

    @patch("docling_step_worker.server.DocumentConverter")
    def test_falls_back_to_default_for_unknown_config(self, mock_converter_cls, caplog):
        server = DoclingStepWorkerServer()

        with caplog.at_level(logging.WARNING):
            converter = server._get_converter("nonexistent")

        assert "default" in server._converters
        assert "Unknown config" in caplog.text

    @patch("docling_step_worker.server.DocumentConverter")
    def test_each_known_config_creates_converter(self, mock_converter_cls):
        server = DoclingStepWorkerServer()

        for config_name in [
            "born_digital",
            "born_digital_with_tables",
            "scanned",
            "default",
        ]:
            converter = server._get_converter(config_name)
            assert config_name in server._converters


class TestComponentCallable:
    """Tests that components can be invoked."""

    @patch("docling_step_worker.server.DocumentConverter")
    @patch("docling_step_worker.server.classify_document")
    @pytest.mark.asyncio
    async def test_classify_component_callable(self, mock_classify, mock_converter_cls):
        mock_classify.return_value = {"recommended_config": "default", "page_count": 1}
        server = DoclingStepWorkerServer()

        result = server.server.get_component("/classify")
        assert result is not None
        component, params = result
        assert component.name == "/classify"

    @patch("docling_step_worker.server.DocumentConverter")
    @patch("docling_step_worker.server.convert_document")
    @pytest.mark.asyncio
    async def test_convert_component_callable(self, mock_convert, mock_converter_cls):
        mock_convert.return_value = {"status": "success", "document": {}}
        server = DoclingStepWorkerServer()

        result = server.server.get_component("/convert")
        assert result is not None
        component, params = result
        assert component.name == "/convert"

    @patch("docling_step_worker.server.DocumentConverter")
    @patch("docling_step_worker.server.chunk_document")
    @pytest.mark.asyncio
    async def test_chunk_component_callable(self, mock_chunk, mock_converter_cls):
        mock_chunk.return_value = {"chunks": [], "chunk_count": 0}
        server = DoclingStepWorkerServer()

        result = server.server.get_component("/chunk")
        assert result is not None
        component, params = result
        assert component.name == "/chunk"
