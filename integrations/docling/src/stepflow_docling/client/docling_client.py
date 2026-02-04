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

"""HTTP client for docling-serve v1 API."""

from __future__ import annotations

import asyncio
import base64
import logging
import os
from enum import Enum
from typing import Any

import httpx
from pydantic import BaseModel, Field

from stepflow_docling.exceptions import (
    DoclingClientError,
    DoclingConversionError,
    DoclingTimeoutError,
)

logger = logging.getLogger(__name__)

# Default timeout for document processing (can be slow for large documents)
DEFAULT_TIMEOUT = 120.0
DEFAULT_POLL_INTERVAL = 2.0
DEFAULT_MAX_POLL_ATTEMPTS = 60


class SourceKind(str, Enum):
    """Source type for document conversion."""

    HTTP = "http"
    BASE64 = "base64"


class ImageExportMode(str, Enum):
    """Image export mode options."""

    PLACEHOLDER = "placeholder"
    EMBEDDED = "embedded"
    REFERENCED = "referenced"


class OcrEngine(str, Enum):
    """OCR engine options."""

    EASYOCR = "easyocr"
    TESSERACT = "tesseract"
    RAPIDOCR = "rapidocr"


class PdfBackend(str, Enum):
    """PDF backend options."""

    DLPARSE_V2 = "dlparse_v2"
    PYPDFIUM2 = "pypdfium2"


class TableMode(str, Enum):
    """Table extraction mode options."""

    FAST = "fast"
    ACCURATE = "accurate"


class ChunkerType(str, Enum):
    """Chunker type options."""

    HYBRID = "hybrid"
    HIERARCHICAL = "hierarchical"


class ExportFormat(str, Enum):
    """Export format options."""

    MARKDOWN = "md"
    JSON = "json"
    HTML = "html"
    TEXT = "text"
    DOCTAGS = "doctags"


class ConversionOptions(BaseModel):
    """Options for document conversion."""

    from_formats: list[str] | None = Field(
        default=None,
        description="Input formats to accept",
    )
    to_formats: list[str] | None = Field(
        default=None,
        description="Output formats to generate",
    )
    image_export_mode: ImageExportMode | None = Field(
        default=None,
        description="How to handle images in output",
    )
    do_ocr: bool | None = Field(
        default=None,
        description="Enable OCR processing",
    )
    force_ocr: bool | None = Field(
        default=None,
        description="Force OCR even on text-based documents",
    )
    ocr_engine: OcrEngine | None = Field(
        default=None,
        description="OCR engine to use",
    )
    ocr_lang: list[str] | None = Field(
        default=None,
        description="OCR language codes",
    )
    pdf_backend: PdfBackend | None = Field(
        default=None,
        description="PDF processing backend",
    )
    table_mode: TableMode | None = Field(
        default=None,
        description="Table extraction mode",
    )
    abort_on_error: bool | None = Field(
        default=None,
        description="Abort on processing errors",
    )


class DocumentSource(BaseModel):
    """A document source for conversion."""

    kind: SourceKind
    url: str | None = None
    base64: str | None = None
    filename: str | None = None


class ConversionRequest(BaseModel):
    """Request body for document conversion."""

    sources: list[DocumentSource]
    options: ConversionOptions | None = None


class ConversionResult(BaseModel):
    """Result of a document conversion."""

    document: dict[str, Any] | None = None
    status: str
    processing_time: float | None = None
    timings: dict[str, Any] | None = None
    error: str | None = None


class AsyncTaskStatus(BaseModel):
    """Status of an async conversion task."""

    task_id: str
    status: str
    result: ConversionResult | None = None
    error: str | None = None


class DoclingServeClient:
    """HTTP client for docling-serve v1 API.

    This client wraps the docling-serve HTTP API, providing methods for
    document conversion, chunking, and export operations.

    Example:
        ```python
        client = DoclingServeClient("http://localhost:5001")

        # Convert a document from URL
        result = await client.convert_from_url("https://example.com/doc.pdf")

        # Convert a document from base64
        result = await client.convert_from_base64(base64_data, filename="doc.pdf")

        # Chunk a converted document
        chunks = await client.chunk(document_data, chunker_type="hybrid")
        ```
    """

    def __init__(
        self,
        base_url: str,
        api_key: str | None = None,
        timeout: float = DEFAULT_TIMEOUT,
    ):
        """Initialize the docling-serve client.

        Args:
            base_url: Base URL of the docling-serve instance
            api_key: Optional API key for authentication
            timeout: Request timeout in seconds
        """
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key or os.environ.get("DOCLING_SERVE_API_KEY")
        self.timeout = timeout

        # Configure httpx client with OpenTelemetry instrumentation if available
        self._client: httpx.AsyncClient | None = None

    async def _get_client(self) -> httpx.AsyncClient:
        """Get or create the HTTP client."""
        if self._client is None or self._client.is_closed:
            headers = {"Content-Type": "application/json"}
            if self.api_key:
                headers["X-Api-Key"] = self.api_key

            self._client = httpx.AsyncClient(
                base_url=self.base_url,
                headers=headers,
                timeout=httpx.Timeout(self.timeout),
            )

            # Try to apply OpenTelemetry instrumentation
            try:
                from opentelemetry.instrumentation.httpx import (
                    HTTPXClientInstrumentor,
                )

                HTTPXClientInstrumentor().instrument_client(self._client)
            except ImportError:
                logger.debug("OpenTelemetry httpx instrumentation not available")

        return self._client

    async def close(self) -> None:
        """Close the HTTP client."""
        if self._client is not None:
            await self._client.aclose()
            self._client = None

    async def __aenter__(self) -> DoclingServeClient:
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Async context manager exit."""
        await self.close()

    async def health(self) -> dict[str, Any]:
        """Check the health of docling-serve.

        Returns:
            Health status response
        """
        client = await self._get_client()
        try:
            response = await client.get("/health")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPError as e:
            raise DoclingClientError(f"Health check failed: {e}") from e

    async def convert(
        self,
        sources: list[DocumentSource],
        options: ConversionOptions | None = None,
    ) -> ConversionResult:
        """Convert documents using the synchronous API.

        Args:
            sources: List of document sources to convert
            options: Optional conversion options

        Returns:
            Conversion result with document content
        """
        client = await self._get_client()

        request = ConversionRequest(sources=sources, options=options)
        request_data = request.model_dump(exclude_none=True)

        try:
            response = await client.post("/v1/convert/source", json=request_data)
            response.raise_for_status()
            return ConversionResult(**response.json())
        except httpx.TimeoutException as e:
            raise DoclingTimeoutError(f"Conversion request timed out: {e}") from e
        except httpx.HTTPStatusError as e:
            raise DoclingConversionError(
                f"Conversion failed with status {e.response.status_code}: "
                f"{e.response.text}"
            ) from e
        except httpx.HTTPError as e:
            raise DoclingClientError(f"Conversion request failed: {e}") from e

    async def convert_from_url(
        self,
        url: str,
        options: ConversionOptions | None = None,
    ) -> ConversionResult:
        """Convert a document from a URL.

        Args:
            url: URL of the document to convert
            options: Optional conversion options

        Returns:
            Conversion result with document content
        """
        source = DocumentSource(kind=SourceKind.HTTP, url=url)
        return await self.convert([source], options)

    async def convert_from_base64(
        self,
        data: str,
        filename: str,
        options: ConversionOptions | None = None,
    ) -> ConversionResult:
        """Convert a document from base64-encoded data.

        Args:
            data: Base64-encoded document content
            filename: Original filename (used for format detection)
            options: Optional conversion options

        Returns:
            Conversion result with document content
        """
        source = DocumentSource(
            kind=SourceKind.BASE64,
            base64=data,
            filename=filename,
        )
        return await self.convert([source], options)

    async def convert_from_bytes(
        self,
        data: bytes,
        filename: str,
        options: ConversionOptions | None = None,
    ) -> ConversionResult:
        """Convert a document from raw bytes.

        Args:
            data: Raw document bytes
            filename: Original filename (used for format detection)
            options: Optional conversion options

        Returns:
            Conversion result with document content
        """
        base64_data = base64.b64encode(data).decode("utf-8")
        return await self.convert_from_base64(base64_data, filename, options)

    async def convert_async(
        self,
        sources: list[DocumentSource],
        options: ConversionOptions | None = None,
    ) -> str:
        """Start an asynchronous document conversion.

        Args:
            sources: List of document sources to convert
            options: Optional conversion options

        Returns:
            Task ID for polling status
        """
        client = await self._get_client()

        request = ConversionRequest(sources=sources, options=options)
        request_data = request.model_dump(exclude_none=True)

        try:
            response = await client.post("/v1/convert/source/async", json=request_data)
            response.raise_for_status()
            result = response.json()
            return result.get("task_id", result.get("id"))
        except httpx.HTTPError as e:
            raise DoclingClientError(f"Async conversion request failed: {e}") from e

    async def poll_task(self, task_id: str) -> AsyncTaskStatus:
        """Poll the status of an async conversion task.

        Args:
            task_id: ID of the task to poll

        Returns:
            Task status including result if complete
        """
        client = await self._get_client()

        try:
            response = await client.get(f"/v1/status/poll/{task_id}")
            response.raise_for_status()
            return AsyncTaskStatus(**response.json())
        except httpx.HTTPError as e:
            raise DoclingClientError(f"Failed to poll task {task_id}: {e}") from e

    async def convert_and_wait(
        self,
        sources: list[DocumentSource],
        options: ConversionOptions | None = None,
        poll_interval: float = DEFAULT_POLL_INTERVAL,
        max_attempts: int = DEFAULT_MAX_POLL_ATTEMPTS,
    ) -> ConversionResult:
        """Convert documents asynchronously and wait for completion.

        Args:
            sources: List of document sources to convert
            options: Optional conversion options
            poll_interval: Seconds between status polls
            max_attempts: Maximum number of poll attempts

        Returns:
            Conversion result with document content
        """
        task_id = await self.convert_async(sources, options)

        for attempt in range(max_attempts):
            status = await self.poll_task(task_id)

            if status.status == "completed":
                if status.result:
                    return status.result
                raise DoclingConversionError("Task completed but no result returned")

            if status.status == "failed":
                raise DoclingConversionError(
                    f"Conversion task failed: {status.error or 'Unknown error'}"
                )

            logger.debug(
                "Task %s status: %s (attempt %d/%d)",
                task_id,
                status.status,
                attempt + 1,
                max_attempts,
            )
            await asyncio.sleep(poll_interval)

        raise DoclingTimeoutError(
            f"Task {task_id} did not complete after {max_attempts} poll attempts"
        )

    async def chunk(
        self,
        sources: list[DocumentSource],
        chunker_type: ChunkerType = ChunkerType.HYBRID,
        options: ConversionOptions | None = None,
    ) -> dict[str, Any]:
        """Convert and chunk documents in one operation.

        Args:
            sources: List of document sources to convert
            chunker_type: Type of chunker to use (hybrid or hierarchical)
            options: Optional conversion options

        Returns:
            Chunked document data
        """
        client = await self._get_client()

        request = ConversionRequest(sources=sources, options=options)
        request_data = request.model_dump(exclude_none=True)

        endpoint = f"/v1/convert/chunked/{chunker_type.value}/source"

        try:
            response = await client.post(endpoint, json=request_data)
            response.raise_for_status()
            return response.json()
        except httpx.HTTPError as e:
            raise DoclingClientError(f"Chunking request failed: {e}") from e

    async def convert_file(
        self,
        file_path: str,
        options: ConversionOptions | None = None,
    ) -> ConversionResult:
        """Convert a local file using multipart upload.

        Args:
            file_path: Path to the file to convert
            options: Optional conversion options

        Returns:
            Conversion result with document content
        """
        client = await self._get_client()

        try:
            with open(file_path, "rb") as f:
                files = {"file": (file_path.split("/")[-1], f)}
                data = {}
                if options:
                    data["options"] = options.model_dump_json(exclude_none=True)

                response = await client.post(
                    "/v1/convert/file",
                    files=files,
                    data=data if data else None,
                )
                response.raise_for_status()
                return ConversionResult(**response.json())
        except httpx.HTTPError as e:
            raise DoclingClientError(f"File conversion failed: {e}") from e
        except FileNotFoundError as e:
            raise DoclingClientError(f"File not found: {file_path}") from e
