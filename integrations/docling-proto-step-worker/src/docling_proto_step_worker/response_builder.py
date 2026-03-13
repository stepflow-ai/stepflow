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

"""Render docling ConversionResult into docling-serve response shapes.

Pure transformation logic — no Stepflow dependency. Produces dicts matching
docling-serve's ExportDocumentResponse and ConvertDocumentResponse.
"""

from __future__ import annotations

import logging
from typing import Any

from docling.datamodel.document import ConversionStatus
from docling_core.types.doc.document import ImageRefMode

logger = logging.getLogger(__name__)

DEFAULT_TO_FORMATS: list[str] = ["markdown"]
DEFAULT_IMAGE_EXPORT_MODE = "embedded"

# docling-serve short name → internal name
FORMAT_NAME_ALIASES: dict[str, str] = {
    "md": "markdown",
}

# Maps format name → (export method, content field, accepts_image_mode)
FORMAT_EXPORT_MAP: dict[str, tuple[str, str, bool]] = {
    "markdown": ("export_to_markdown", "md_content", True),
    "html": ("export_to_html", "html_content", True),
    "text": ("export_to_text", "text_content", False),
    "json": ("export_to_dict", "json_content", False),
    "doctags": ("export_to_doctags", "doctags_content", False),
}

# Add yaml and html_split_page if docling supports them
try:
    from docling_core.types.doc.document import DoclingDocument as _DocCheck

    if hasattr(_DocCheck, "export_to_yaml"):
        FORMAT_EXPORT_MAP["yaml"] = ("export_to_yaml", "yaml_content", False)
    if hasattr(_DocCheck, "export_to_html_split_page"):
        FORMAT_EXPORT_MAP["html_split_page"] = (
            "export_to_html_split_page",
            "html_split_page_content",
            True,
        )
except ImportError:
    pass


def normalize_format_name(name: str) -> str:
    """Normalize a docling-serve short format name to internal name.

    Maps aliases like ``"md"`` to ``"markdown"``. Unrecognized names
    are returned unchanged so that the caller can log and skip them.
    """
    return FORMAT_NAME_ALIASES.get(name, name)


def _make_error_item(message: str) -> dict[str, str]:
    """Create a structured error item matching docling-serve's ErrorItem."""
    return {
        "component_type": "response_builder",
        "module_name": "docling_proto_step_worker",
        "error_message": message,
    }


def build_export_document(
    doc: Any,
    filename: str,
    to_formats: list[str] | None = None,
    image_export_mode: str | None = None,
) -> dict[str, Any]:
    """Render a DoclingDocument into an ExportDocumentResponse-shaped dict.

    Args:
        doc: A DoclingDocument instance (from ConversionResult.document)
        filename: Source filename
        to_formats: List of format names to export (default: ["markdown"])
        image_export_mode: Image reference mode for markdown/html exports

    Returns:
        Dict with filename and requested *_content fields.
        An internal ``_export_errors`` list is attached for the caller to merge.
    """
    if to_formats is None:
        to_formats = DEFAULT_TO_FORMATS
    if image_export_mode is None:
        image_export_mode = DEFAULT_IMAGE_EXPORT_MODE

    try:
        image_mode = ImageRefMode(image_export_mode)
    except ValueError:
        logger.warning(
            "Invalid image_export_mode %r, falling back to embedded",
            image_export_mode,
        )
        image_mode = ImageRefMode.EMBEDDED
    export_errors: list[dict[str, str]] = []

    result: dict[str, Any] = {"filename": filename}

    for raw_fmt in to_formats:
        fmt = normalize_format_name(raw_fmt)
        entry = FORMAT_EXPORT_MAP.get(fmt)
        if entry is None:
            logger.warning("Unknown export format %r, skipping", fmt)
            continue

        method_name, field_name, accepts_image_mode = entry

        try:
            method = getattr(doc, method_name)
            if accepts_image_mode:
                content = method(image_mode=image_mode)
            else:
                content = method()
            result[field_name] = content
        except Exception as exc:
            logger.warning("Export to %s failed: %s", fmt, exc)
            result[field_name] = None
            export_errors.append(_make_error_item(f"Export to {fmt} failed: {exc}"))

    result["_export_errors"] = export_errors
    return result


def build_convert_response(
    doc: Any,
    filename: str,
    elapsed_seconds: float,
    to_formats: list[str] | None = None,
    image_export_mode: str | None = None,
    errors: list[dict[str, str]] | None = None,
    timings: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Build a ConvertDocumentResponse-shaped dict.

    Args:
        doc: A DoclingDocument instance
        filename: Source filename
        elapsed_seconds: Total processing time in seconds
        to_formats: Export formats (passed to build_export_document)
        image_export_mode: Image mode (passed to build_export_document)
        errors: Pre-existing errors to include
        timings: Timing breakdown dict

    Returns:
        Dict matching docling-serve's ConvertDocumentResponse shape.
    """
    document = build_export_document(doc, filename, to_formats, image_export_mode)

    # Extract and merge export errors into top-level errors list
    export_errors = document.pop("_export_errors", [])
    all_errors = list(errors or []) + export_errors

    return {
        "document": document,
        "status": ConversionStatus.SUCCESS.value,
        "errors": all_errors,
        "processing_time": elapsed_seconds,
        "timings": timings or {},
    }
