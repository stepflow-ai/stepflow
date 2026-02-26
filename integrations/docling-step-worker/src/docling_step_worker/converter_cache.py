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

"""LRU cache of DocumentConverter instances keyed by options hash.

Matches docling-serve's caching strategy: one converter per distinct
options combination, evicting least-recently-used when cache is full.
"""

from __future__ import annotations

import hashlib
import json
import logging
from typing import Any

from docling.datamodel.base_models import InputFormat
from docling.datamodel.pipeline_options import (
    PdfPipelineOptions,
    TableFormerMode,
    TableStructureOptions,
)
from docling.document_converter import DocumentConverter, FormatOption, PdfFormatOption
from docling.pipeline.standard_pdf_pipeline import StandardPdfPipeline

from docling_step_worker.config import PIPELINE_CONFIGS

logger = logging.getLogger(__name__)

# Match docling-serve's DOCLING_SERVE_CACHE_SIZE_OPTIONS default
DEFAULT_CACHE_SIZE = 8

# Map docling-serve's ocr_engine string to the appropriate OCR options class
OCR_ENGINE_MAP = {
    "easyocr": "EasyOcrOptions",
    "tesseract": "TesseractCliOcrOptions",
    "tesserocr": "TesseractOcrOptions",
    "ocrmac": "OcrMacOptions",
    "rapidocr": "RapidOcrOptions",
}

# Map docling-serve's table_mode to TableFormerMode
TABLE_MODE_MAP = {
    "fast": TableFormerMode.FAST,
    "accurate": TableFormerMode.ACCURATE,
}


def _options_hash(options: dict[str, Any]) -> str:
    """Produce a stable hash for a dict of pipeline options."""
    canonical = json.dumps(options, sort_keys=True, default=str)
    return hashlib.sha256(canonical.encode()).hexdigest()[:16]


def build_pipeline_options(request_options: dict[str, Any]) -> PdfPipelineOptions:
    """Build PdfPipelineOptions from a request options dict.

    Maps docling-serve's flat option names to docling's nested options hierarchy.
    Only sets values that are explicitly provided; unset values use docling defaults.
    """
    kwargs: dict[str, Any] = {}

    if "do_ocr" in request_options:
        kwargs["do_ocr"] = request_options["do_ocr"]
    if "force_ocr" in request_options:
        # force_ocr may not be available in all docling versions
        if (
            hasattr(PdfPipelineOptions.model_fields, "force_ocr")
            or "force_ocr" in PdfPipelineOptions.model_fields
        ):
            kwargs["force_ocr"] = request_options["force_ocr"]
        else:
            logger.warning("force_ocr not supported in this docling version")
    if "do_table_structure" in request_options:
        kwargs["do_table_structure"] = request_options["do_table_structure"]
    if (
        "document_timeout" in request_options
        and request_options["document_timeout"] is not None
    ):
        kwargs["document_timeout"] = request_options["document_timeout"]
    if "images_scale" in request_options:
        kwargs["images_scale"] = request_options["images_scale"]
    if "include_images" in request_options:
        kwargs["generate_picture_images"] = request_options["include_images"]

    # Table mode
    table_mode = request_options.get("table_mode")
    if table_mode and table_mode in TABLE_MODE_MAP:
        kwargs["table_structure_options"] = TableStructureOptions(
            mode=TABLE_MODE_MAP[table_mode]
        )

    # OCR engine
    ocr_engine = request_options.get("ocr_engine")
    if ocr_engine and ocr_engine in OCR_ENGINE_MAP:
        ocr_class_name = OCR_ENGINE_MAP[ocr_engine]
        try:
            from docling.datamodel import pipeline_options as po

            ocr_cls = getattr(po, ocr_class_name)
            ocr_opts = ocr_cls()
            ocr_lang = request_options.get("ocr_lang")
            if ocr_lang and hasattr(ocr_opts, "lang"):
                ocr_opts.lang = ocr_lang
            kwargs["ocr_options"] = ocr_opts
        except (AttributeError, ImportError):
            logger.warning("OCR engine '%s' not available, using default", ocr_engine)

    return PdfPipelineOptions(**kwargs)


def build_converter(request_options: dict[str, Any]) -> DocumentConverter:
    """Build a DocumentConverter from request options.

    Handles both PdfPipelineOptions and top-level converter options
    like from_formats and pdf_backend.
    """
    pipeline_options = build_pipeline_options(request_options)

    pipeline_cls = StandardPdfPipeline

    format_options: dict[InputFormat, FormatOption] = {
        InputFormat.PDF: PdfFormatOption(
            pipeline_cls=pipeline_cls,
            pipeline_options=pipeline_options,
        )
    }

    # from_formats → allowed_formats
    allowed_formats = None
    from_formats = request_options.get("from_formats")
    if from_formats:
        format_map = {f.value: f for f in InputFormat}
        allowed_formats = [format_map[f] for f in from_formats if f in format_map]

    return DocumentConverter(
        format_options=format_options,
        allowed_formats=allowed_formats,
    )


def _build_converter_from_config(config_name: str) -> DocumentConverter:
    """Build a DocumentConverter from a named pipeline config."""
    pipeline_options = PIPELINE_CONFIGS[config_name]
    return DocumentConverter(
        format_options={
            InputFormat.PDF: PdfFormatOption(
                pipeline_cls=StandardPdfPipeline,
                pipeline_options=pipeline_options,
            )
        }
    )


class ConverterCache:
    """LRU cache for DocumentConverter instances.

    Supports both named pipeline configs (from classify step)
    and per-request options (from direct callers).
    """

    def __init__(self, max_size: int = DEFAULT_CACHE_SIZE):
        self._cache: dict[str, DocumentConverter] = {}
        self._access_order: list[str] = []
        self._max_size = max_size

    def get_by_config_name(self, config_name: str) -> DocumentConverter:
        """Get converter for a named pipeline config."""
        if config_name not in PIPELINE_CONFIGS:
            logger.warning(
                "Unknown config '%s', falling back to 'default'", config_name
            )
            config_name = "default"
        key = f"config:{config_name}"
        if key not in self._cache:
            self._evict_if_full()
            logger.info(
                "Initializing DocumentConverter for config '%s'...", config_name
            )
            self._cache[key] = _build_converter_from_config(config_name)
            logger.info("DocumentConverter for '%s' initialized.", config_name)
        self._touch(key)
        return self._cache[key]

    def get_by_options(self, options: dict[str, Any]) -> DocumentConverter:
        """Get converter for per-request options."""
        key = f"opts:{_options_hash(options)}"
        if key not in self._cache:
            self._evict_if_full()
            logger.info("Building DocumentConverter for options hash %s", key)
            self._cache[key] = build_converter(options)
        self._touch(key)
        return self._cache[key]

    @property
    def size(self) -> int:
        """Number of cached converters."""
        return len(self._cache)

    def _touch(self, key: str):
        if key in self._access_order:
            self._access_order.remove(key)
        self._access_order.append(key)

    def _evict_if_full(self):
        while len(self._cache) >= self._max_size and self._access_order:
            oldest = self._access_order.pop(0)
            self._cache.pop(oldest, None)

    def clear(self):
        self._cache.clear()
        self._access_order.clear()
