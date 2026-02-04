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

    def test_extract_from_path_string(self, server):
        """Test extracting from path field with URL string."""
        input_data = {"path": "https://en.wikipedia.org/wiki/Artificial_intelligence"}
        files = server._extract_files(input_data)
        assert len(files) == 1
        assert files[0]["url"] == "https://en.wikipedia.org/wiki/Artificial_intelligence"

    def test_extract_from_path_list(self, server):
        """Test extracting from path field with URL list."""
        input_data = {
            "path": [
                "https://example.com/doc1.pdf",
                "https://example.com/doc2.pdf",
            ]
        }
        files = server._extract_files(input_data)
        assert len(files) == 2
        assert files[0]["url"] == "https://example.com/doc1.pdf"
        assert files[1]["url"] == "https://example.com/doc2.pdf"

    def test_extract_from_url_string(self, server):
        """Test extracting from url field."""
        input_data = {"url": "https://example.com/doc.pdf"}
        files = server._extract_files(input_data)
        assert len(files) == 1
        assert files[0]["url"] == "https://example.com/doc.pdf"

    def test_extract_nested_input_path(self, server):
        """Test extracting from nested input.input.path (Langflow workflow format).

        In Langflow workflows, the component input structure is:
        - input.template: Langflow component metadata
        - input.input: Actual runtime values with resolved references

        Example workflow structure:
          input:
            template:
              _type: Component
              path:
                _input_type: FileInput
                value: ''  # Empty in template
            input:
              path: "https://example.com/doc.pdf"  # Resolved at runtime
        """
        input_data = {
            "template": {
                "_type": "Component",
                "path": {
                    "_input_type": "FileInput",
                    "value": "",  # Empty in template
                },
            },
            "input": {
                "api_url": None,
                "path": "https://en.wikipedia.org/wiki/Artificial_intelligence",
            },
        }
        files = server._extract_files(input_data)
        assert len(files) == 1
        assert files[0]["url"] == "https://en.wikipedia.org/wiki/Artificial_intelligence"

    def test_extract_nested_input_url(self, server):
        """Test extracting from nested input.input.url (Langflow workflow format)."""
        input_data = {
            "template": {"_type": "Component"},
            "input": {
                "url": "https://example.com/doc.pdf",
            },
        }
        files = server._extract_files(input_data)
        assert len(files) == 1
        assert files[0]["url"] == "https://example.com/doc.pdf"

    def test_extract_prefers_top_level_over_nested(self, server):
        """Test that top-level path is preferred over nested."""
        input_data = {
            "path": "https://top-level.com/doc.pdf",
            "input": {
                "path": "https://nested.com/doc.pdf",
            },
        }
        files = server._extract_files(input_data)
        assert len(files) == 1
        # Top-level should be found first
        assert files[0]["url"] == "https://top-level.com/doc.pdf"


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


class TestExtractContentForExport:
    """Tests for extracting content from already-converted documents."""

    @pytest.fixture
    def server(self):
        return StepflowDoclingServer()

    def test_extract_from_files_list(self, server):
        """Test extracting content from files list (DoclingRemoteComponent output)."""
        data_inputs = {
            "files": [
                {"content": "# Document 1\nSome content", "filename": "doc1.md"},
                {"content": "# Document 2\nMore content", "filename": "doc2.md"},
            ],
            "status": "success",
        }
        content = server._extract_content_for_export(data_inputs, "Markdown")
        assert "# Document 1" in content
        assert "# Document 2" in content
        assert "Some content" in content
        assert "More content" in content

    def test_extract_from_direct_content(self, server):
        """Test extracting content from dict with direct content."""
        data_inputs = {"content": "# Direct Content\nText here"}
        content = server._extract_content_for_export(data_inputs, "Markdown")
        assert content == "# Direct Content\nText here"

    def test_extract_from_md_content(self, server):
        """Test extracting content from md_content field."""
        data_inputs = {"md_content": "# Markdown Content"}
        content = server._extract_content_for_export(data_inputs, "Markdown")
        assert content == "# Markdown Content"

    def test_extract_from_list_of_dicts(self, server):
        """Test extracting content from list of document dicts."""
        data_inputs = [
            {"content": "Content 1"},
            {"content": "Content 2"},
        ]
        content = server._extract_content_for_export(data_inputs, "Markdown")
        assert "Content 1" in content
        assert "Content 2" in content

    def test_extract_from_list_of_strings(self, server):
        """Test extracting content from list of strings."""
        data_inputs = ["Text 1", "Text 2"]
        content = server._extract_content_for_export(data_inputs, "Markdown")
        assert "Text 1" in content
        assert "Text 2" in content

    def test_extract_empty_input(self, server):
        """Test extracting from empty input returns empty string."""
        content = server._extract_content_for_export({}, "Markdown")
        assert content == ""

    def test_extract_nested_result_structure(self, server):
        """Test extracting from DoclingRemoteComponent result structure.

        DoclingRemoteComponent returns: {result: {files: [...], status: ...}}
        After Stepflow resolves $step path 'result', downstream gets: {files: [...]}
        """
        # This is what ExportDoclingDocument receives after path resolution
        data_inputs = {
            "files": [
                {
                    "filename": "wiki_article",
                    "content": "# Artificial Intelligence\n\nAI is...",
                    "docling_document": {"source": "https://en.wikipedia.org/..."},
                    "status": "success",
                    "processing_time": 6.5,
                }
            ],
            "status": "success",
        }
        content = server._extract_content_for_export(data_inputs, "Markdown")
        assert "# Artificial Intelligence" in content
        assert "AI is..." in content


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
        # Output is wrapped in 'result' for Langflow compatibility
        assert "result" in output
        assert "files" in output["result"]
        assert len(output["result"]["files"]) == 1
        assert output["result"]["files"][0]["content"] == "# Test"

    def test_format_chunk_output(self, server):
        """Test formatting chunk output."""
        result = {
            "chunks": [
                {"text": "Chunk 1", "metadata": {"page": 1}},
                {"text": "Chunk 2", "metadata": {"page": 2}},
            ]
        }
        output = server._format_chunk_output(result)
        # Output is wrapped in 'result' for Langflow compatibility
        assert "result" in output
        assert "data" in output["result"]
        assert "chunks" in output["result"]
        assert len(output["result"]["chunks"]) == 2

    def test_format_export_output(self, server):
        """Test formatting export output."""
        result = MagicMock()
        result.document = {"md_content": "# Exported content"}

        output = server._format_export_output(result, "Markdown")
        # Output is wrapped in 'result' for Langflow compatibility
        assert "result" in output
        assert output["result"]["format"] == "Markdown"
        assert output["result"]["content"] == "# Exported content"


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
