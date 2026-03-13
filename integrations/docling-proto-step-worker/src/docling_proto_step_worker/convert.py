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

"""Document conversion component.

Wraps docling's DocumentConverter.convert() directly.
Supports both named pipeline configs (from classify step) and
per-request options matching docling-serve's ConvertDocumentsOptions.
"""

from __future__ import annotations

import asyncio
import logging
import time
from typing import Any

from docling.datamodel.document import ConversionStatus
from docling.document_converter import DocumentConverter
from stepflow_py.worker import StepflowContext

from docling_proto_step_worker.blob_utils import (
    bytes_to_document_stream,
    get_document_bytes,
    put_document_blob,
)
from docling_proto_step_worker.converter_cache import ConverterCache
from docling_proto_step_worker.metrics import convert_duration, documents_processed
from docling_proto_step_worker.response_builder import (
    _make_error_item,
    build_convert_response,
    normalize_format_name,
)

logger = logging.getLogger(__name__)


def _parse_page_range(page_range: int | str | None) -> tuple[int, int] | None:
    """Parse a page_range value into a (start, end) tuple for converter.convert().

    Accepts:
        - int: treated as max_num_pages (returns None, caller uses max_num_pages)
        - str like "1-10": parsed as (start, end) tuple
        - None: no page range constraint

    Returns:
        A (start, end) tuple, or None if not applicable.
        For int values, returns None (caller should use max_num_pages instead).
    """
    if page_range is None:
        return None
    if isinstance(page_range, int):
        return None  # handled via max_num_pages in caller
    if isinstance(page_range, str) and "-" in page_range:
        parts = page_range.split("-", 1)
        try:
            return (int(parts[0]), int(parts[1]))
        except (ValueError, IndexError):
            logger.warning("Invalid page_range format '%s', ignoring", page_range)
            return None
    logger.warning("Unsupported page_range value '%s', ignoring", page_range)
    return None


def _run_conversion(
    converter: DocumentConverter,
    data: bytes,
    filename: str,
    to_formats: list[str] | None = None,
    image_export_mode: str | None = None,
    page_range: int | str | None = None,
) -> dict[str, Any]:
    """Run synchronous docling conversion.

    Args:
        converter: Pre-configured DocumentConverter instance
        data: Raw document bytes
        filename: Filename for the document stream
        to_formats: Export formats for response rendering
        image_export_mode: Image reference mode for markdown/html
        page_range: Page range constraint (int for max pages, or "start-end" string)

    Returns:
        Dict with document (ExportDocumentResponse shape), document_dict,
        status, processing_time, errors, timings
    """
    stream = bytes_to_document_stream(data, filename)

    convert_kwargs: dict[str, Any] = {"source": stream}
    if page_range is not None:
        parsed = _parse_page_range(page_range)
        if parsed is not None:
            convert_kwargs["page_range"] = parsed
        elif isinstance(page_range, int):
            convert_kwargs["max_num_pages"] = page_range

    start = time.monotonic()
    result = converter.convert(**convert_kwargs)
    elapsed_seconds = time.monotonic() - start

    doc = result.document
    doc_dict = doc.export_to_dict()

    response = build_convert_response(
        doc,
        filename,
        elapsed_seconds,
        to_formats=to_formats,
        image_export_mode=image_export_mode,
    )
    response["document_dict"] = doc_dict

    return response


async def convert_document(
    input_data: dict[str, Any],
    context: StepflowContext,
    converter_cache: ConverterCache,
) -> dict[str, Any]:
    """Convert a document using docling's DocumentConverter directly.

    Supports both named pipeline configs (from classify step) and
    per-request options matching docling-serve's ConvertDocumentsOptions.
    Per-request options take precedence over named configs.

    Input:
        source: blob reference or URL
        source_kind: "blob" or "url"
        pipeline_config: named config from classify step (default: "default")
        options: dict of docling-serve compatible conversion options
        to_formats: list of export formats (default: ["markdown"])
        image_export_mode: image reference mode (default: "embedded")

    Output:
        document: dict (ExportDocumentResponse shape with *_content fields)
        document_dict: dict (DoclingDocument via export_to_dict())
        status: "success" or "failure"
        errors: list of ErrorItem dicts
        processing_time: float (seconds)
        timings: dict
    """
    source = input_data.get("source", "")
    source_kind = input_data.get("source_kind", "blob")
    filename = input_data.get("filename", "document.pdf")

    # Per-request options (nested dict) take precedence
    request_options = input_data.get("options")
    pipeline_config = input_data.get("pipeline_config", "default")

    page_range = None
    if request_options:
        converter = converter_cache.get_by_options(request_options)
        # to_formats and image_export_mode from options take precedence
        to_formats = request_options.get("to_formats", input_data.get("to_formats"))
        image_export_mode = request_options.get(
            "image_export_mode", input_data.get("image_export_mode")
        )
        page_range = request_options.get("page_range")
    else:
        converter = converter_cache.get_by_config_name(pipeline_config)
        to_formats = input_data.get("to_formats")
        image_export_mode = input_data.get("image_export_mode")

    # Normalize format names (e.g. "md" → "markdown")
    if to_formats:
        to_formats = [normalize_format_name(f) for f in to_formats]

    try:
        doc_bytes = await get_document_bytes(source, source_kind)
    except Exception as e:
        logger.error("Failed to retrieve document for conversion: %s", e)
        return {
            "document": None,
            "document_dict": None,
            "status": ConversionStatus.FAILURE.value,
            "errors": [_make_error_item(f"Failed to retrieve document: {e}")],
            "processing_time": 0.0,
            "timings": {},
        }

    try:
        # Run sync conversion in thread to avoid blocking event loop
        result = await asyncio.to_thread(
            _run_conversion,
            converter,
            doc_bytes,
            filename,
            to_formats,
            image_export_mode,
            page_range,
        )
    except Exception as e:
        logger.error("Document conversion failed: %s", e)
        documents_processed.add(
            1,
            {
                "format": filename.rsplit(".", 1)[-1] if "." in filename else "unknown",
                "pipeline_config": pipeline_config,
                "status": "failure",
            },
        )
        return {
            "document": None,
            "document_dict": None,
            "status": ConversionStatus.FAILURE.value,
            "errors": [_make_error_item(f"Conversion failed: {e}")],
            "processing_time": 0.0,
            "timings": {},
        }

    # Record conversion metrics
    fmt = filename.rsplit(".", 1)[-1] if "." in filename else "unknown"
    status = result.get("status", "unknown")
    documents_processed.add(
        1, {"format": fmt, "pipeline_config": pipeline_config, "status": status}
    )
    convert_duration.record(
        result.get("processing_time", 0.0), {"pipeline_config": pipeline_config}
    )

    # Store document_dict in blob store for downstream consumption
    try:
        blob_ref = await put_document_blob(result["document_dict"])
        result["document_blob_ref"] = blob_ref
    except Exception as e:
        logger.warning("Failed to store document blob: %s", e)
        # Non-fatal — document_dict is still in the result

    return result
