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

"""Document classification component.

Lightweight document probing using pypdfium2 to determine
optimal pipeline configuration. Does NOT run the full conversion pipeline.
"""

from __future__ import annotations

import asyncio
import logging
from typing import Any

from stepflow_py.worker import StepflowContext

from docling_step_worker.blob_utils import get_document_bytes
from docling_step_worker.metrics import classify_page_count

logger = logging.getLogger(__name__)


def _classify_pdf_bytes(data: bytes) -> dict[str, Any]:
    """Synchronous PDF classification using pypdfium2.

    Probes the PDF to determine page count, text layer presence,
    and heuristic table detection.

    Args:
        data: Raw PDF bytes

    Returns:
        Classification dict with page_count, has_text_layer,
        estimated_tables, format, and recommended_config.
    """
    import pypdfium2 as pdfium

    result: dict[str, Any] = {
        "page_count": 0,
        "has_text_layer": False,
        "estimated_tables": 0,
        "format": "pdf",
        "recommended_config": "default",
    }

    try:
        pdf = pdfium.PdfDocument(data)
        result["page_count"] = len(pdf)

        if len(pdf) > 0:
            # Check first page for text layer
            page = pdf[0]
            textpage = page.get_textpage()
            text = textpage.get_text_bounded()
            has_text = bool(text and text.strip())
            result["has_text_layer"] = has_text

            # Heuristic table detection: look for tab-separated or
            # grid-like structures in the text of all pages (sample first 3)
            table_indicators = 0
            pages_to_check = min(len(pdf), 3)
            for i in range(pages_to_check):
                p = pdf[i]
                tp = p.get_textpage()
                page_text = tp.get_text_bounded()
                if page_text:
                    # Count lines with multiple tab/pipe separators
                    for line in page_text.split("\n"):
                        if line.count("\t") >= 2 or line.count("|") >= 2:
                            table_indicators += 1

            result["estimated_tables"] = table_indicators

            # Determine recommended config
            if not has_text:
                result["recommended_config"] = "scanned"
            elif table_indicators > 0:
                result["recommended_config"] = "born_digital_with_tables"
            else:
                result["recommended_config"] = "born_digital"

        pdf.close()
    except Exception as e:
        logger.warning("PDF classification failed, using defaults: %s", e)
        # Return safe defaults — classification failures should not block pipeline
        result["recommended_config"] = "default"

    return result


async def classify_document(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Probe a document to determine optimal pipeline configuration.

    Input:
        source: blob reference (blob:sha256:...) or URL
        source_kind: "blob" or "url"

    Output:
        page_count: int
        has_text_layer: bool
        estimated_tables: int (heuristic)
        format: str ("pdf", "docx", etc.)
        recommended_config: str ("born_digital", "born_digital_with_tables", "scanned")
    """
    source = input_data.get("source", "")
    source_kind = input_data.get("source_kind", "blob")

    try:
        doc_bytes = await get_document_bytes(source, source_kind)
    except Exception as e:
        logger.warning("Failed to retrieve document for classification: %s", e)
        return {
            "page_count": 0,
            "has_text_layer": False,
            "estimated_tables": 0,
            "format": "unknown",
            "recommended_config": "default",
        }

    # Run sync classification in thread to avoid blocking event loop
    try:
        result = await asyncio.to_thread(_classify_pdf_bytes, doc_bytes)
    except Exception as e:
        logger.warning("Classification failed: %s", e)
        return {
            "page_count": 0,
            "has_text_layer": False,
            "estimated_tables": 0,
            "format": "pdf",
            "recommended_config": "default",
        }

    classify_page_count.record(result["page_count"])

    return result
