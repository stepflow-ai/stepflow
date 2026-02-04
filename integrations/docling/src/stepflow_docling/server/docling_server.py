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

"""Stepflow component server for Docling integration.

This server registers components matching Langflow's Docling class names exactly,
enabling routing-based component execution. The translator emits paths based on
component class names, and routing configuration determines whether components
execute on langflow-worker or docling-worker.
"""

from __future__ import annotations

import asyncio
import base64
import logging
import os
from typing import Any

import msgspec

from stepflow_py.worker import StepflowContext, StepflowServer



from stepflow_docling.client.docling_client import (
    ChunkerType,
    ConversionOptions,
    DoclingServeClient,
    DocumentSource,
    ExportFormat,
    OcrEngine,
    SourceKind,
)
from stepflow_docling.exceptions import DoclingValidationError

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    force=True,
)

logger = logging.getLogger(__name__)


class StepflowDoclingServer:
    """Stepflow component server for Docling document processing.

    This server registers components matching Langflow's class names exactly:
    - DoclingInlineComponent
    - DoclingRemoteComponent
    - ChunkDoclingDocument
    - ExportDoclingDocument

    The component names must match Langflow's class names for routing to work.
    When Stepflow routes a request to this server, it strips the route prefix
    and sends just the component name (e.g., /DoclingInlineComponent).

    Example configuration:
        ```yaml
        plugins:
          docling_k8s:
            type: stepflow
            transport: http
            url: "http://docling-load-balancer.stepflow.svc.cluster.local:8080"

        routes:
          "/langflow/core/lfx/components/docling/{*component}":
            - plugin: docling_k8s
        ```
    """

    def __init__(
        self,
        docling_serve_url: str | None = None,
        api_key: str | None = None,
    ):
        """Initialize the Docling component server.

        Args:
            docling_serve_url: URL of the docling-serve instance.
                              Defaults to DOCLING_SERVE_URL env var or localhost:5001
            api_key: Optional API key for docling-serve authentication.
                    Defaults to DOCLING_SERVE_API_KEY env var
        """
        self.server = StepflowServer()

        # Initialize docling-serve client
        self.docling_url = docling_serve_url or os.environ.get(
            "DOCLING_SERVE_URL", "http://localhost:5001"
        )
        self.api_key = api_key or os.environ.get("DOCLING_SERVE_API_KEY")

        logger.info(
            "Initializing Docling server with docling-serve at %s", self.docling_url
        )

        # Register all components
        self._register_components()

    def _get_client(self) -> DoclingServeClient:
        """Create a new docling-serve client instance."""
        return DoclingServeClient(
            base_url=self.docling_url,
            api_key=self.api_key,
        )

    def _register_components(self) -> None:
        """Register all Docling components matching Langflow class names."""

        @self.server.component(name="DoclingInlineComponent")
        async def docling_inline(
            input_data: Any, context: StepflowContext
        ) -> Any:
            """Process documents using docling-serve (inline mode).

            This component processes documents through docling-serve, handling
            both URL-based and file-based inputs. In the sidecar deployment
            pattern, docling-serve runs locally on the same pod.

            Input fields (from Langflow):
                - files: List of file data with content (Data/DataFrame)
                - pipeline: Processing pipeline configuration
                - ocr_engine: OCR engine to use (easyocr, tesseract, rapidocr)

            Output:
                - files: List of files with DoclingDocument data attached
            """
            return await self._process_documents(input_data, context)

        @self.server.component(name="DoclingRemoteComponent")
        async def docling_remote(
            input_data: Any, context: StepflowContext
        ) -> Any:
            """Process documents via remote docling-serve API.

            This component processes documents through a remote docling-serve
            instance, supporting async processing for large documents.

            Input fields (from Langflow):
                - files: List of file data with content
                - api_url: URL of the docling-serve instance (optional, uses default)
                - max_concurrency: Maximum concurrent requests
                - max_poll_timeout: Maximum time to wait for async tasks

            Output:
                - files: List of files with DoclingDocument data attached
            """
            return await self._process_documents(input_data, context)

        @self.server.component(name="ChunkDoclingDocument")
        async def chunk_docling(
            input_data: Any, context: StepflowContext
        ) -> Any:
            """Split DoclingDocument into semantic chunks.

            This component takes previously converted documents and chunks them
            for RAG pipelines using either hybrid or hierarchical chunking.

            Input fields (from Langflow):
                - data_inputs: Data/DataFrame containing DoclingDocument
                - chunker: Chunker type (HybridChunker or HierarchicalChunker)
                - max_tokens: Maximum tokens per chunk
                - tokenizer: Tokenizer configuration

            Output:
                - DataFrame of chunks with text and metadata
            """
            return await self._chunk_documents(input_data, context)

        @self.server.component(name="ExportDoclingDocument")
        async def export_docling(
            input_data: Any, context: StepflowContext
        ) -> Any:
            """Export DoclingDocument to various formats.

            This component exports converted documents to Markdown, HTML,
            plain text, or DocTags format.

            Input fields (from Langflow):
                - data_inputs: Data/DataFrame containing DoclingDocument
                - export_format: Target format (Markdown, HTML, Plaintext, DocTags)
                - image_mode: How to handle images in export

            Output:
                - Data or DataFrame with exported content
            """
            return await self._export_documents(input_data, context)

    async def _process_documents(
        self, input_data: dict[str, Any], context: StepflowContext
    ) -> dict[str, Any]:
        """Process documents through docling-serve.

        This method handles the conversion of documents from various input
        formats to structured DoclingDocument output.
        """
        # Log input structure at INFO level for debugging
        logger.info("Processing documents with input keys: %s", list(input_data.keys()) if isinstance(input_data, dict) else type(input_data).__name__)
        if isinstance(input_data, dict):
            for key, value in input_data.items():
                if isinstance(value, dict):
                    logger.info("  input_data['%s'] keys: %s", key, list(value.keys()))
                elif isinstance(value, str) and len(value) > 100:
                    logger.info("  input_data['%s']: <string of length %d>", key, len(value))
                else:
                    logger.info("  input_data['%s']: %s", key, repr(value)[:200])

        # Extract files from input
        files = self._extract_files(input_data)
        if not files:
            raise DoclingValidationError("No files provided for processing")

        # Extract conversion options
        options = self._build_conversion_options(input_data)

        # Build document sources
        sources = self._build_sources(files)

        # Process through docling-serve
        async with self._get_client() as client:
            result = await client.convert(sources, options)

        # Format output for Langflow compatibility
        return self._format_conversion_output(result, files)

    async def _chunk_documents(
        self, input_data: dict[str, Any], context: StepflowContext
    ) -> dict[str, Any]:
        """Chunk documents using docling-serve."""
        logger.info("Chunking documents with input keys: %s", list(input_data.keys()) if isinstance(input_data, dict) else type(input_data).__name__)

        # Check for nested Langflow workflow format (input.input.data_inputs)
        nested_input = input_data.get("input", {})
        if isinstance(nested_input, dict) and nested_input:
            logger.info("Found nested 'input' key with keys: %s", list(nested_input.keys()))

        # Extract document data - check both top-level and nested
        data_inputs = (
            input_data.get("data_inputs")
            or input_data.get("data")
            or nested_input.get("data_inputs")
            or nested_input.get("data")
            or []
        )
        if not data_inputs:
            logger.error("No data_inputs found. input_data keys: %s, nested_input keys: %s",
                        list(input_data.keys()), list(nested_input.keys()) if nested_input else [])
            raise DoclingValidationError("No document data provided for chunking")

        logger.info("Found data_inputs: %s", type(data_inputs).__name__)

        # Determine chunker type - check both top-level and nested
        chunker_config = input_data.get("chunker") or nested_input.get("chunker") or {}
        chunker_type = ChunkerType.HYBRID
        if isinstance(chunker_config, dict):
            chunker_name = chunker_config.get("type", "HybridChunker")
            if "Hierarchical" in chunker_name:
                chunker_type = ChunkerType.HIERARCHICAL
        elif isinstance(chunker_config, str) and "Hierarchical" in chunker_config:
            chunker_type = ChunkerType.HIERARCHICAL

        # Build sources from document data
        sources = self._build_sources_from_data(data_inputs)

        # Chunk through docling-serve
        async with self._get_client() as client:
            result = await client.chunk(sources, chunker_type)

        # Format output as DataFrame-compatible structure
        return self._format_chunk_output(result)

    async def _export_documents(
        self, input_data: dict[str, Any], context: StepflowContext
    ) -> dict[str, Any]:
        """Export documents to specified format.

        Note: Export functionality is typically handled by the document
        conversion itself by specifying to_formats. This component provides
        a separate step for format conversion of already-processed documents.
        """
        logger.info("Exporting documents with input keys: %s", list(input_data.keys()) if isinstance(input_data, dict) else type(input_data).__name__)

        # Check for nested Langflow workflow format (input.input.data_inputs)
        nested_input = input_data.get("input", {})
        if isinstance(nested_input, dict) and nested_input:
            logger.info("Found nested 'input' key with keys: %s", list(nested_input.keys()))

        # Extract document data - check both top-level and nested
        data_inputs = (
            input_data.get("data_inputs")
            or input_data.get("data")
            or nested_input.get("data_inputs")
            or nested_input.get("data")
            or nested_input.get("df")  # DataFrame input
            or []
        )
        if not data_inputs:
            logger.error("No data_inputs found. input_data keys: %s, nested_input keys: %s",
                        list(input_data.keys()), list(nested_input.keys()) if nested_input else [])
            raise DoclingValidationError("No document data provided for export")

        logger.info("Found data_inputs: %s", type(data_inputs).__name__)
        logger.info("data_inputs content: %s", repr(data_inputs)[:500])

        # Determine export format - check both top-level and nested
        export_format_str = (
            input_data.get("export_format")
            or nested_input.get("export_format")
            or "Markdown"
        )

        # Extract content from already-converted document data
        # The data_inputs comes from DoclingRemoteComponent output which has:
        # {files: [{content, docling_document, ...}], status: ...}
        content = self._extract_content_for_export(data_inputs, export_format_str)

        # Return formatted output with result wrapper
        return {
            "result": {
                "data": {
                    "content": content,
                    "format": export_format_str,
                },
                "content": content,
                "format": export_format_str,
            }
        }

    def _extract_content_for_export(
        self, data_inputs: Any, export_format: str
    ) -> str:
        """Extract content from already-converted document data for export.

        The data_inputs can be:
        - dict with 'files' list containing converted documents
        - dict with 'content' directly
        - list of document dicts
        """
        content_parts = []

        # Handle dict input
        if isinstance(data_inputs, dict):
            # Check for files list (from DoclingRemoteComponent output)
            files = data_inputs.get("files", [])
            if files:
                for f in files:
                    if isinstance(f, dict):
                        # Get content based on export format
                        doc_content = (
                            f.get("content")
                            or f.get("md_content")
                            or f.get("text")
                            or ""
                        )
                        if doc_content:
                            content_parts.append(doc_content)
            else:
                # Direct content
                doc_content = (
                    data_inputs.get("content")
                    or data_inputs.get("md_content")
                    or data_inputs.get("text")
                    or ""
                )
                if doc_content:
                    content_parts.append(doc_content)

        # Handle list input
        elif isinstance(data_inputs, list):
            for item in data_inputs:
                if isinstance(item, dict):
                    doc_content = (
                        item.get("content")
                        or item.get("md_content")
                        or item.get("text")
                        or ""
                    )
                    if doc_content:
                        content_parts.append(doc_content)
                elif isinstance(item, str):
                    content_parts.append(item)

        return "\n\n".join(content_parts)

    def _extract_files(self, input_data: dict[str, Any]) -> list[dict[str, Any]]:
        """Extract file data from input.

        Supports multiple input formats:
        - files: List of file data dicts (standard format)
        - file: Single file data dict
        - path: URL string or list of URLs (lfx DoclingRemoteComponent format)
        - url: URL string (direct URL input)
        - input.path: Nested Langflow workflow format (path under 'input' key)
        """
        # Log input structure for debugging
        logger.debug("_extract_files input_data keys: %s", list(input_data.keys()))

        # Check for nested Langflow workflow format first
        # Workflow structure: input.template + input.input.path
        nested_input = input_data.get("input", {})
        if isinstance(nested_input, dict) and nested_input:
            logger.debug("Found nested 'input' key with keys: %s", list(nested_input.keys()))

        files = input_data.get("files", [])
        if not files:
            # Try alternative field names
            files = input_data.get("file", [])
            if files and not isinstance(files, list):
                files = [files]

        if not files:
            # Handle lfx-style 'path' input (URL string or list)
            # Check both top-level and nested input.path
            path = input_data.get("path") or nested_input.get("path")
            if path:
                logger.debug("Found path: %s (type: %s)", path, type(path).__name__)
                if isinstance(path, str):
                    # Single URL/path string
                    files = [{"url": path}]
                elif isinstance(path, list):
                    # List of URLs/paths
                    files = [{"url": p} if isinstance(p, str) else p for p in path]

        if not files:
            # Handle direct 'url' input (top-level or nested)
            url = input_data.get("url") or nested_input.get("url")
            if url:
                logger.debug("Found url: %s (type: %s)", url, type(url).__name__)
                if isinstance(url, str):
                    files = [{"url": url}]
                elif isinstance(url, list):
                    files = [{"url": u} if isinstance(u, str) else u for u in url]

        logger.debug("Extracted %d files: %s", len(files), files[:3] if files else [])
        return files if isinstance(files, list) else [files] if files else []

    def _build_conversion_options(
        self, input_data: dict[str, Any]
    ) -> ConversionOptions | None:
        """Build conversion options from input data."""
        options_dict: dict[str, Any] = {}

        # OCR configuration
        ocr_engine = input_data.get("ocr_engine")
        if ocr_engine:
            engine_mapping = {
                "easyocr": OcrEngine.EASYOCR,
                "tesseract": OcrEngine.TESSERACT,
                "rapidocr": OcrEngine.RAPIDOCR,
            }
            options_dict["ocr_engine"] = engine_mapping.get(
                ocr_engine.lower(), OcrEngine.EASYOCR
            )

        do_ocr = input_data.get("do_ocr")
        if do_ocr is not None:
            options_dict["do_ocr"] = bool(do_ocr)

        # Output format
        output_format = input_data.get("output_format", input_data.get("to_format"))
        if output_format:
            options_dict["to_formats"] = (
                [output_format] if isinstance(output_format, str) else output_format
            )

        return ConversionOptions(**options_dict) if options_dict else None

    def _build_sources(self, files: list[dict[str, Any]]) -> list[DocumentSource]:
        """Build document sources from file data."""
        sources = []

        for file_data in files:
            # Handle different file data formats
            if isinstance(file_data, dict):
                # Check for URL
                url = file_data.get("url") or file_data.get("path")
                if url and (url.startswith("http://") or url.startswith("https://")):
                    sources.append(DocumentSource(kind=SourceKind.HTTP, url=url))
                    continue

                # Check for base64 content
                content = file_data.get("content") or file_data.get("data")
                if content:
                    filename = file_data.get(
                        "filename", file_data.get("name", "document")
                    )
                    # If content is bytes, encode to base64
                    if isinstance(content, bytes):
                        content = base64.b64encode(content).decode("utf-8")
                    sources.append(
                        DocumentSource(
                            kind=SourceKind.BASE64,
                            base64=content,
                            filename=filename,
                        )
                    )
            elif isinstance(file_data, str):
                # Assume it's a URL or path
                if file_data.startswith("http://") or file_data.startswith("https://"):
                    sources.append(DocumentSource(kind=SourceKind.HTTP, url=file_data))

        return sources

    def _build_sources_from_data(
        self, data_inputs: list[Any] | dict[str, Any]
    ) -> list[DocumentSource]:
        """Build document sources from data inputs (for chunking/export)."""
        # Handle both list and single data input
        if isinstance(data_inputs, dict):
            data_inputs = [data_inputs]

        sources = []
        for data in data_inputs:
            if isinstance(data, dict):
                # Check for document content that needs re-processing
                content = data.get("content") or data.get("text") or data.get("data")
                if content:
                    filename = data.get("filename", data.get("name", "document.md"))
                    if isinstance(content, bytes):
                        content = base64.b64encode(content).decode("utf-8")
                    elif isinstance(content, str) and not self._is_base64(content):
                        # Plain text - encode to base64
                        content = base64.b64encode(content.encode("utf-8")).decode(
                            "utf-8"
                        )
                    sources.append(
                        DocumentSource(
                            kind=SourceKind.BASE64,
                            base64=content,
                            filename=filename,
                        )
                    )

        return sources

    def _is_base64(self, s: str) -> bool:
        """Check if a string is base64 encoded."""
        try:
            if len(s) % 4 != 0:
                return False
            base64.b64decode(s, validate=True)
            return True
        except Exception:
            return False

    def _format_conversion_output(
        self, result: Any, original_files: list[dict[str, Any]]
    ) -> dict[str, Any]:
        """Format conversion result for Langflow compatibility.

        Output is wrapped in a 'result' field to match Langflow conventions.
        Downstream steps expect: $step: docling_step, path: result
        """
        output_files = []

        if hasattr(result, "document") and result.document:
            doc = result.document
            output_files.append(
                {
                    "filename": doc.get("filename", "document"),
                    "content": doc.get("md_content", doc.get("content", "")),
                    "docling_document": doc,
                    "status": result.status,
                    "processing_time": result.processing_time,
                }
            )
        else:
            # Return result as-is if it's already structured
            if isinstance(result, dict):
                # Wrap in result for Langflow compatibility
                return {"result": result}

        # Wrap output in 'result' field for Langflow compatibility
        return {
            "result": {
                "files": output_files,
                "status": getattr(result, "status", "success"),
            }
        }

    def _format_chunk_output(self, result: dict[str, Any]) -> dict[str, Any]:
        """Format chunking result as DataFrame-compatible structure.

        Output is wrapped in a 'result' field to match Langflow conventions.
        """
        chunks = result.get("chunks", [])

        # Format as DataFrame-compatible output, wrapped in 'result'
        return {
            "result": {
                "data": {
                    "columns": ["text", "metadata", "chunk_index"],
                    "data": [
                        [
                            chunk.get("text", ""),
                            chunk.get("metadata", {}),
                            idx,
                        ]
                        for idx, chunk in enumerate(chunks)
                    ],
                },
                "chunks": chunks,
            }
        }

    def _format_export_output(self, result: Any, export_format: str) -> dict[str, Any]:
        """Format export result.

        Output is wrapped in a 'result' field to match Langflow conventions.
        """
        content = ""
        if hasattr(result, "document") and result.document:
            doc = result.document
            # Get content based on format
            content = doc.get("md_content") or doc.get("content") or ""

        return {
            "result": {
                "data": {
                    "content": content,
                    "format": export_format,
                },
                "content": content,
                "format": export_format,
            }
        }

    def run(self, *args: Any, **kwargs: Any) -> None:
        """Run the component server.

        This method starts the Stepflow HTTP server, which handles
        JSON-RPC communication with the Stepflow runtime.
        """
        # Apply nest_asyncio to allow nested event loops
        # This is needed for compatibility with some async code patterns
        import nest_asyncio

        nest_asyncio.apply()

        logger.info("Starting Docling component server...")
        asyncio.run(self.server.run(*args, **kwargs))


if __name__ == "__main__":
    """Run the Docling component server."""
    server = StepflowDoclingServer()
    server.run()
