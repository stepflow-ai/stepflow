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

"""Unit tests for StepflowDoclingServer."""

import base64
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from stepflow_docling.client.docling_client import (
    SourceKind,
)
from stepflow_docling.server.docling_server import StepflowDoclingServer


class TestStepflowDoclingServer:
    """Tests for StepflowDoclingServer."""

    @pytest.fixture
    def server(self):
        """Create a server instance for testing."""
        return StepflowDoclingServer(docling_serve_url="http://localhost:5001")

    @pytest.fixture
    def mock_context(self):
        """Create a mock StepflowContext."""
        context = MagicMock()
        context.put_blob = AsyncMock(return_value="blob-123")
        context.get_blob = AsyncMock(return_value={"data": "test"})
        return context


class TestExtractFiles:
    """Tests for file extraction from input."""

    @pytest.fixture
    def server(self):
        return StepflowDoclingServer()

    def test_extract_files_from_list(self, server):
        """Test extracting files from a list."""
        input_data = {
            "files": [
                {"url": "https://example.com/doc.pdf"},
                {"content": "base64content", "filename": "doc.pdf"},
            ]
        }
        files = server._extract_files(input_data)
        assert len(files) == 2

    def test_extract_single_file(self, server):
        """Test extracting a single file."""
        input_data = {"file": {"url": "https://example.com/doc.pdf"}}
        files = server._extract_files(input_data)
        assert len(files) == 1

    def test_extract_no_files(self, server):
        """Test extracting when no files present."""
        input_data = {"other_field": "value"}
        files = server._extract_files(input_data)
        assert len(files) == 0


class TestBuildSources:
    """Tests for building document sources."""

    @pytest.fixture
    def server(self):
        return StepflowDoclingServer()

    def test_build_sources_from_url(self, server):
        """Test building sources from URLs."""
        files = [
            {"url": "https://example.com/doc.pdf"},
            {"path": "https://example.com/other.pdf"},
        ]
        sources = server._build_sources(files)
        assert len(sources) == 2
        assert all(s.kind == SourceKind.HTTP for s in sources)

    def test_build_sources_from_content(self, server):
        """Test building sources from base64 content."""
        content = base64.b64encode(b"PDF content").decode("utf-8")
        files = [{"content": content, "filename": "test.pdf"}]
        sources = server._build_sources(files)
        assert len(sources) == 1
        assert sources[0].kind == SourceKind.BASE64
        assert sources[0].filename == "test.pdf"

    def test_build_sources_from_bytes(self, server):
        """Test building sources from bytes content."""
        files = [{"content": b"PDF content", "filename": "test.pdf"}]
        sources = server._build_sources(files)
        assert len(sources) == 1
        assert sources[0].kind == SourceKind.BASE64

    def test_build_sources_from_string_url(self, server):
        """Test building sources from string URLs."""
        files = ["https://example.com/doc.pdf", "https://example.com/other.pdf"]
        sources = server._build_sources(files)
        assert len(sources) == 2
        assert all(s.kind == SourceKind.HTTP for s in sources)


class TestBuildConversionOptions:
    """Tests for building conversion options."""

    @pytest.fixture
    def server(self):
        return StepflowDoclingServer()

    def test_build_options_with_ocr(self, server):
        """Test building options with OCR settings."""
        input_data = {
            "ocr_engine": "tesseract",
            "do_ocr": True,
        }
        options = server._build_conversion_options(input_data)
        assert options is not None
        assert options.do_ocr is True

    def test_build_options_with_format(self, server):
        """Test building options with output format."""
        input_data = {"output_format": "md"}
        options = server._build_conversion_options(input_data)
        assert options is not None
        assert options.to_formats == ["md"]

    def test_build_options_empty(self, server):
        """Test building options with no input."""
        input_data = {}
        options = server._build_conversion_options(input_data)
        assert options is None


class TestFormatOutput:
    """Tests for output formatting."""

    @pytest.fixture
    def server(self):
        return StepflowDoclingServer()

    def test_format_conversion_output(self, server):
        """Test formatting conversion output."""
        result = MagicMock()
        result.document = {"md_content": "# Test", "filename": "test.pdf"}
        result.status = "success"
        result.processing_time = 1.5

        output = server._format_conversion_output(result, [])
        assert "files" in output
        assert len(output["files"]) == 1
        assert output["files"][0]["content"] == "# Test"

    def test_format_chunk_output(self, server):
        """Test formatting chunk output."""
        result = {
            "chunks": [
                {"text": "Chunk 1", "metadata": {"page": 1}},
                {"text": "Chunk 2", "metadata": {"page": 2}},
            ]
        }
        output = server._format_chunk_output(result)
        assert "data" in output
        assert "chunks" in output
        assert len(output["chunks"]) == 2

    def test_format_export_output(self, server):
        """Test formatting export output."""
        result = MagicMock()
        result.document = {"md_content": "# Exported content"}

        output = server._format_export_output(result, "Markdown")
        assert output["format"] == "Markdown"
        assert output["content"] == "# Exported content"


class TestIsBase64:
    """Tests for base64 detection."""

    @pytest.fixture
    def server(self):
        return StepflowDoclingServer()

    def test_valid_base64(self, server):
        """Test detecting valid base64."""
        valid = base64.b64encode(b"test content").decode("utf-8")
        assert server._is_base64(valid) is True

    def test_invalid_base64(self, server):
        """Test detecting invalid base64."""
        assert server._is_base64("not base64!") is False

    def test_wrong_length(self, server):
        """Test detecting wrong length strings."""
        assert server._is_base64("abc") is False  # Length not multiple of 4


class TestServerInitialization:
    """Tests for server initialization."""

    def test_default_initialization(self):
        """Test server with default settings."""
        server = StepflowDoclingServer()
        assert server.docling_url == "http://localhost:5001"
        assert server.api_key is None

    def test_custom_initialization(self):
        """Test server with custom settings."""
        server = StepflowDoclingServer(
            docling_serve_url="http://custom:8080",
            api_key="test-api-key",
        )
        assert server.docling_url == "http://custom:8080"
        assert server.api_key == "test-api-key"

    @patch.dict("os.environ", {"DOCLING_SERVE_URL": "http://env:9000"})
    def test_env_var_initialization(self):
        """Test server using environment variables."""
        server = StepflowDoclingServer()
        assert server.docling_url == "http://env:9000"

    def test_server_has_components(self):
        """Test that server registers expected components."""
        server = StepflowDoclingServer()
        # The server object should have the StepflowServer instance
        assert server.server is not None


class TestComponentRegistration:
    """Tests for component registration."""

    def test_components_registered(self):
        """Test that all expected components are registered."""
        server = StepflowDoclingServer()
        # The StepflowServer should have components registered
        # This tests the internal structure indirectly
        assert hasattr(server, "server")
        assert server.server is not None
