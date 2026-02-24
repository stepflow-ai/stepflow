"""Document conversion component.

Wraps docling's DocumentConverter.convert() directly.
The converter is pre-configured with pipeline options selected
by config name in the server layer.
"""

from __future__ import annotations

import asyncio
import logging
import time
from typing import Any

from docling.document_converter import DocumentConverter
from stepflow_py.worker import StepflowContext

from docling_step_worker.blob_utils import (
    bytes_to_document_stream,
    get_document_bytes,
    put_document_blob,
)

logger = logging.getLogger(__name__)


def _run_conversion(
    converter: DocumentConverter, data: bytes, filename: str
) -> dict[str, Any]:
    """Run synchronous docling conversion.

    Args:
        converter: Pre-configured DocumentConverter instance
        data: Raw document bytes
        filename: Filename for the document stream

    Returns:
        Dict with document, status, page_count, table_count, processing_time_ms
    """
    stream = bytes_to_document_stream(data, filename)
    start = time.monotonic()
    result = converter.convert(source=stream)
    elapsed_ms = int((time.monotonic() - start) * 1000)

    doc = result.document
    doc_dict = doc.export_to_dict()

    # Count pages and tables from the DoclingDocument
    page_count = len(doc_dict.get("pages", {}))
    table_count = len(doc_dict.get("tables", []))

    return {
        "document": doc_dict,
        "status": "success",
        "page_count": page_count,
        "table_count": table_count,
        "processing_time_ms": elapsed_ms,
    }


async def convert_document(
    input_data: dict[str, Any],
    context: StepflowContext,
    converter: DocumentConverter,
) -> dict[str, Any]:
    """Convert a document using docling's DocumentConverter directly.

    The converter is pre-configured with the appropriate pipeline options
    (selected by pipeline_config name in the server layer). This function
    does NOT accept per-request pipeline_options — DocumentConverter.convert()
    doesn't support that.

    Input:
        source: blob reference or URL
        source_kind: "blob" or "url"

    Output:
        document: dict (DoclingDocument via export_to_dict())
        status: "success" or "error"
        page_count: int
        table_count: int
        processing_time_ms: int
    """
    source = input_data.get("source", "")
    source_kind = input_data.get("source_kind", "blob")
    filename = input_data.get("filename", "document.pdf")

    try:
        doc_bytes = await get_document_bytes(source, source_kind)
    except Exception as e:
        logger.error("Failed to retrieve document for conversion: %s", e)
        return {
            "document": None,
            "status": "error",
            "error": f"Failed to retrieve document: {e}",
            "page_count": 0,
            "table_count": 0,
            "processing_time_ms": 0,
        }

    try:
        # Run sync conversion in thread to avoid blocking event loop
        result = await asyncio.to_thread(
            _run_conversion, converter, doc_bytes, filename
        )
    except Exception as e:
        logger.error("Document conversion failed: %s", e)
        return {
            "document": None,
            "status": "error",
            "error": f"Conversion failed: {e}",
            "page_count": 0,
            "table_count": 0,
            "processing_time_ms": 0,
        }

    # Store document in blob store for downstream consumption
    try:
        blob_ref = await put_document_blob(result["document"])
        result["document_blob_ref"] = blob_ref
    except Exception as e:
        logger.warning("Failed to store document blob: %s", e)
        # Non-fatal — document dict is still in the result

    return result
