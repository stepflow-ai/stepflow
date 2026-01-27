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

"""Unit tests for DoclingServeClient."""

import base64

import httpx
import pytest
import respx

from stepflow_docling.client.docling_client import (
    ChunkerType,
    ConversionOptions,
    ConversionResult,
    DoclingServeClient,
    DocumentSource,
    ExportFormat,
    OcrEngine,
    SourceKind,
)
from stepflow_docling.exceptions import (
    DoclingClientError,
    DoclingConversionError,
    DoclingTimeoutError,
)


class TestDocumentSource:
    """Tests for DocumentSource model."""

    def test_http_source(self):
        """Test creating an HTTP document source."""
        source = DocumentSource(kind=SourceKind.HTTP, url="https://example.com/doc.pdf")
        assert source.kind == SourceKind.HTTP
        assert source.url == "https://example.com/doc.pdf"
        assert source.base64 is None

    def test_base64_source(self):
        """Test creating a base64 document source."""
        content = base64.b64encode(b"test document").decode("utf-8")
        source = DocumentSource(
            kind=SourceKind.BASE64, base64=content, filename="test.pdf"
        )
        assert source.kind == SourceKind.BASE64
        assert source.base64 == content
        assert source.filename == "test.pdf"


class TestConversionOptions:
    """Tests for ConversionOptions model."""

    def test_default_options(self):
        """Test default conversion options."""
        options = ConversionOptions()
        assert options.to_formats is None
        assert options.do_ocr is None
        assert options.ocr_engine is None

    def test_custom_options(self):
        """Test custom conversion options."""
        options = ConversionOptions(
            to_formats=["md", "json"],
            do_ocr=True,
            ocr_engine=OcrEngine.EASYOCR,
        )
        assert options.to_formats == ["md", "json"]
        assert options.do_ocr is True
        assert options.ocr_engine == OcrEngine.EASYOCR


class TestConversionResult:
    """Tests for ConversionResult model."""

    def test_successful_result(self):
        """Test a successful conversion result."""
        result = ConversionResult(
            document={"md_content": "# Test", "filename": "test.pdf"},
            status="success",
            processing_time=1.5,
        )
        assert result.status == "success"
        assert result.document is not None
        assert result.document["md_content"] == "# Test"
        assert result.processing_time == 1.5
        assert result.error is None

    def test_failed_result(self):
        """Test a failed conversion result."""
        result = ConversionResult(
            status="failed",
            error="Document format not supported",
        )
        assert result.status == "failed"
        assert result.error == "Document format not supported"
        assert result.document is None


class TestDoclingServeClient:
    """Tests for DoclingServeClient."""

    @pytest.fixture
    def client(self):
        """Create a client instance for testing."""
        return DoclingServeClient(base_url="http://localhost:5001")

    @pytest.mark.asyncio
    @respx.mock
    async def test_health_check(self, client):
        """Test health check endpoint."""
        respx.get("http://localhost:5001/health").mock(
            return_value=httpx.Response(200, json={"status": "healthy"})
        )

        async with client:
            result = await client.health()
            assert result == {"status": "healthy"}

    @pytest.mark.asyncio
    @respx.mock
    async def test_convert_from_url(self, client):
        """Test converting a document from URL."""
        respx.post("http://localhost:5001/v1/convert/source").mock(
            return_value=httpx.Response(
                200,
                json={
                    "document": {"md_content": "# Converted content"},
                    "status": "success",
                    "processing_time": 2.5,
                },
            )
        )

        async with client:
            result = await client.convert_from_url("https://example.com/doc.pdf")
            assert result.status == "success"
            assert result.document is not None
            assert result.document["md_content"] == "# Converted content"

    @pytest.mark.asyncio
    @respx.mock
    async def test_convert_from_base64(self, client):
        """Test converting a document from base64."""
        content = base64.b64encode(b"PDF content").decode("utf-8")

        respx.post("http://localhost:5001/v1/convert/source").mock(
            return_value=httpx.Response(
                200,
                json={
                    "document": {"md_content": "# From base64"},
                    "status": "success",
                    "processing_time": 1.0,
                },
            )
        )

        async with client:
            result = await client.convert_from_base64(content, filename="test.pdf")
            assert result.status == "success"

    @pytest.mark.asyncio
    @respx.mock
    async def test_convert_with_options(self, client):
        """Test conversion with custom options."""
        respx.post("http://localhost:5001/v1/convert/source").mock(
            return_value=httpx.Response(
                200,
                json={"status": "success", "processing_time": 1.5},
            )
        )

        options = ConversionOptions(
            to_formats=["md"],
            do_ocr=True,
            ocr_engine=OcrEngine.TESSERACT,
        )

        async with client:
            result = await client.convert_from_url(
                "https://example.com/scan.pdf", options=options
            )
            assert result.status == "success"

    @pytest.mark.asyncio
    @respx.mock
    async def test_chunk_documents(self, client):
        """Test chunking documents."""
        respx.post("http://localhost:5001/v1/convert/chunked/hybrid/source").mock(
            return_value=httpx.Response(
                200,
                json={
                    "chunks": [
                        {"text": "Chunk 1", "metadata": {}},
                        {"text": "Chunk 2", "metadata": {}},
                    ]
                },
            )
        )

        source = DocumentSource(kind=SourceKind.HTTP, url="https://example.com/doc.pdf")

        async with client:
            result = await client.chunk([source], ChunkerType.HYBRID)
            assert "chunks" in result
            assert len(result["chunks"]) == 2

    @pytest.mark.asyncio
    @respx.mock
    async def test_convert_timeout(self, client):
        """Test timeout handling."""
        respx.post("http://localhost:5001/v1/convert/source").mock(
            side_effect=httpx.TimeoutException("Request timed out")
        )

        async with client:
            with pytest.raises(DoclingTimeoutError):
                await client.convert_from_url("https://example.com/large.pdf")

    @pytest.mark.asyncio
    @respx.mock
    async def test_convert_http_error(self, client):
        """Test HTTP error handling."""
        respx.post("http://localhost:5001/v1/convert/source").mock(
            return_value=httpx.Response(500, text="Internal Server Error")
        )

        async with client:
            with pytest.raises(DoclingConversionError):
                await client.convert_from_url("https://example.com/doc.pdf")

    @pytest.mark.asyncio
    @respx.mock
    async def test_health_check_failure(self, client):
        """Test health check failure."""
        respx.get("http://localhost:5001/health").mock(
            side_effect=httpx.ConnectError("Connection refused")
        )

        async with client:
            with pytest.raises(DoclingClientError):
                await client.health()

    @pytest.mark.asyncio
    @respx.mock
    async def test_async_conversion_poll(self, client):
        """Test async conversion with polling."""
        # First call starts the async task
        respx.post("http://localhost:5001/v1/convert/source/async").mock(
            return_value=httpx.Response(200, json={"task_id": "task-123"})
        )

        # Poll returns completed
        respx.get("http://localhost:5001/v1/status/poll/task-123").mock(
            return_value=httpx.Response(
                200,
                json={
                    "task_id": "task-123",
                    "status": "completed",
                    "result": {
                        "document": {"md_content": "# Async result"},
                        "status": "success",
                        "processing_time": 5.0,
                    },
                },
            )
        )

        source = DocumentSource(
            kind=SourceKind.HTTP, url="https://example.com/large.pdf"
        )

        async with client:
            result = await client.convert_and_wait([source])
            assert result.status == "success"
            assert result.document is not None

    def test_client_initialization(self):
        """Test client initialization with various parameters."""
        # Basic initialization
        client = DoclingServeClient(base_url="http://localhost:5001")
        assert client.base_url == "http://localhost:5001"
        assert client.api_key is None

        # With API key
        client = DoclingServeClient(
            base_url="http://localhost:5001", api_key="test-key"
        )
        assert client.api_key == "test-key"

        # With custom timeout
        client = DoclingServeClient(base_url="http://localhost:5001", timeout=300.0)
        assert client.timeout == 300.0

    def test_client_strips_trailing_slash(self):
        """Test that trailing slashes are removed from base URL."""
        client = DoclingServeClient(base_url="http://localhost:5001/")
        assert client.base_url == "http://localhost:5001"


class TestEnums:
    """Tests for enum types."""

    def test_source_kind(self):
        """Test SourceKind enum values."""
        assert SourceKind.HTTP.value == "http"
        assert SourceKind.BASE64.value == "base64"

    def test_ocr_engine(self):
        """Test OcrEngine enum values."""
        assert OcrEngine.EASYOCR.value == "easyocr"
        assert OcrEngine.TESSERACT.value == "tesseract"
        assert OcrEngine.RAPIDOCR.value == "rapidocr"

    def test_chunker_type(self):
        """Test ChunkerType enum values."""
        assert ChunkerType.HYBRID.value == "hybrid"
        assert ChunkerType.HIERARCHICAL.value == "hierarchical"

    def test_export_format(self):
        """Test ExportFormat enum values."""
        assert ExportFormat.MARKDOWN.value == "md"
        assert ExportFormat.JSON.value == "json"
        assert ExportFormat.HTML.value == "html"
        assert ExportFormat.TEXT.value == "text"
        assert ExportFormat.DOCTAGS.value == "doctags"
