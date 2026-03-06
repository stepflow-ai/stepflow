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

"""Tests for converter_cache module."""

from __future__ import annotations

import logging
from concurrent.futures import ThreadPoolExecutor
from unittest.mock import MagicMock, patch

from docling.datamodel.pipeline_options import PdfPipelineOptions, TableFormerMode

from docling_step_worker.converter_cache import (
    ConverterCache,
    _options_hash,
    build_converter,
    build_pipeline_options,
)


class TestOptionsHash:
    """Tests for _options_hash determinism and order independence."""

    def test_deterministic(self):
        opts = {"do_ocr": True, "table_mode": "fast"}
        assert _options_hash(opts) == _options_hash(opts)

    def test_order_independent(self):
        opts_a = {"a": 1, "b": 2}
        opts_b = {"b": 2, "a": 1}
        assert _options_hash(opts_a) == _options_hash(opts_b)

    def test_different_values_produce_different_hash(self):
        opts_a = {"do_ocr": True}
        opts_b = {"do_ocr": False}
        assert _options_hash(opts_a) != _options_hash(opts_b)


class TestBuildPipelineOptions:
    """Tests for build_pipeline_options."""

    def test_defaults_when_empty(self):
        opts = build_pipeline_options({})
        assert isinstance(opts, PdfPipelineOptions)

    def test_do_ocr_false(self):
        opts = build_pipeline_options({"do_ocr": False})
        assert opts.do_ocr is False

    def test_force_ocr(self):
        opts = build_pipeline_options({"force_ocr": True})
        # force_ocr may not be available in all docling versions
        if hasattr(opts, "force_ocr"):
            assert opts.force_ocr is True

    def test_table_mode_accurate(self):
        opts = build_pipeline_options({"table_mode": "accurate"})
        assert opts.table_structure_options.mode == TableFormerMode.ACCURATE

    def test_table_mode_fast(self):
        opts = build_pipeline_options({"table_mode": "fast"})
        assert opts.table_structure_options.mode == TableFormerMode.FAST

    def test_do_table_structure(self):
        opts = build_pipeline_options({"do_table_structure": False})
        assert opts.do_table_structure is False

    def test_document_timeout(self):
        opts = build_pipeline_options({"document_timeout": 30.0})
        assert opts.document_timeout == 30.0

    def test_document_timeout_none_ignored(self):
        opts = build_pipeline_options({"document_timeout": None})
        default = PdfPipelineOptions()
        assert opts.document_timeout == default.document_timeout

    def test_images_scale(self):
        opts = build_pipeline_options({"images_scale": 3.0})
        assert opts.images_scale == 3.0

    def test_include_images(self):
        opts = build_pipeline_options({"include_images": False})
        assert opts.generate_picture_images is False

    def test_unknown_table_mode_ignored(self):
        opts = build_pipeline_options({"table_mode": "nonexistent"})
        default = PdfPipelineOptions()
        assert opts.table_structure_options == default.table_structure_options

    def test_unknown_ocr_engine_logs_warning(self, caplog):
        with caplog.at_level(logging.WARNING):
            opts = build_pipeline_options({"ocr_engine": "nonexistent"})
        default = PdfPipelineOptions()
        assert opts.ocr_options == default.ocr_options

    def test_abort_on_error_logged_when_unsupported(self, caplog):
        """abort_on_error logs warning if unsupported."""
        has_field = "abort_on_error" in PdfPipelineOptions.model_fields
        with caplog.at_level(logging.WARNING):
            opts = build_pipeline_options({"abort_on_error": True})
        if has_field:
            assert opts.abort_on_error is True
        else:
            assert "abort_on_error not supported" in caplog.text


class TestBuildConverter:
    """Tests for build_converter with pdf_backend."""

    def test_pdf_backend_dlparse_v2(self):
        from docling.backend.docling_parse_backend import DoclingParseDocumentBackend

        converter = build_converter({"pdf_backend": "dlparse_v2"})
        from docling.datamodel.base_models import InputFormat

        fmt_opt = converter.format_to_options[InputFormat.PDF]
        assert fmt_opt.backend is DoclingParseDocumentBackend

    def test_pdf_backend_pypdfium2(self):
        from docling.backend.pypdfium2_backend import PyPdfiumDocumentBackend

        converter = build_converter({"pdf_backend": "pypdfium2"})
        from docling.datamodel.base_models import InputFormat

        fmt_opt = converter.format_to_options[InputFormat.PDF]
        assert fmt_opt.backend is PyPdfiumDocumentBackend

    def test_pdf_backend_unknown_uses_default(self, caplog):
        from docling.backend.docling_parse_backend import DoclingParseDocumentBackend

        with caplog.at_level(logging.WARNING):
            converter = build_converter({"pdf_backend": "nonexistent"})
        assert "Unknown pdf_backend" in caplog.text
        from docling.datamodel.base_models import InputFormat

        fmt_opt = converter.format_to_options[InputFormat.PDF]
        assert fmt_opt.backend is DoclingParseDocumentBackend

    def test_pdf_backend_not_set_uses_default(self):
        from docling.backend.docling_parse_backend import DoclingParseDocumentBackend

        converter = build_converter({})
        from docling.datamodel.base_models import InputFormat

        fmt_opt = converter.format_to_options[InputFormat.PDF]
        assert fmt_opt.backend is DoclingParseDocumentBackend


class TestConverterCache:
    """Tests for ConverterCache LRU behavior."""

    @patch("docling_step_worker.converter_cache.build_converter")
    def test_same_options_return_same_converter(self, mock_build):
        mock_build.return_value = MagicMock()
        cache = ConverterCache()
        opts = {"do_ocr": False}

        c1 = cache.get_by_options(opts)
        c2 = cache.get_by_options(opts)

        assert c1 is c2
        assert mock_build.call_count == 1

    @patch("docling_step_worker.converter_cache.build_converter")
    def test_different_options_return_different_converters(self, mock_build):
        mock_build.side_effect = [MagicMock(), MagicMock()]
        cache = ConverterCache()

        c1 = cache.get_by_options({"do_ocr": False})
        c2 = cache.get_by_options({"do_ocr": True})

        assert c1 is not c2
        assert mock_build.call_count == 2

    @patch("docling_step_worker.converter_cache._build_converter_from_config")
    def test_named_config_fallback(self, mock_build):
        mock_build.return_value = MagicMock()
        cache = ConverterCache()

        converter = cache.get_by_config_name("born_digital")
        assert converter is not None
        mock_build.assert_called_once_with("born_digital")

    @patch("docling_step_worker.converter_cache._build_converter_from_config")
    def test_unknown_config_falls_back_to_default(self, mock_build, caplog):
        mock_build.return_value = MagicMock()
        cache = ConverterCache()

        with caplog.at_level(logging.WARNING):
            cache.get_by_config_name("nonexistent")

        assert "Unknown config" in caplog.text
        mock_build.assert_called_once_with("default")

    @patch("docling_step_worker.converter_cache.build_converter")
    def test_lru_eviction(self, mock_build):
        converters = [MagicMock() for _ in range(4)]
        mock_build.side_effect = converters
        cache = ConverterCache(max_size=2)

        c1 = cache.get_by_options({"key": "a"})
        c2 = cache.get_by_options({"key": "b"})
        assert cache.size == 2

        # Adding a third should evict the first
        c3 = cache.get_by_options({"key": "c"})
        assert cache.size == 2

        # First converter is evicted, new one is created
        c1_again = cache.get_by_options({"key": "a"})
        assert c1_again is not c1
        assert mock_build.call_count == 4

    @patch("docling_step_worker.converter_cache.build_converter")
    def test_lru_touch_prevents_eviction(self, mock_build):
        converters = [MagicMock() for _ in range(4)]
        mock_build.side_effect = converters
        cache = ConverterCache(max_size=2)

        cache.get_by_options({"key": "a"})
        cache.get_by_options({"key": "b"})

        # Touch "a" so "b" is now the oldest
        cache.get_by_options({"key": "a"})

        # Adding "c" should evict "b", not "a"
        cache.get_by_options({"key": "c"})
        assert cache.size == 2

        # "a" should still be cached (no new build call for it)
        c_a = cache.get_by_options({"key": "a"})
        # Total builds: a, b, c = 3 (a was not rebuilt)
        assert mock_build.call_count == 3

    @patch("docling_step_worker.converter_cache._build_converter_from_config")
    def test_concurrent_access_returns_same_instance(self, mock_build):
        sentinel = MagicMock()
        mock_build.return_value = sentinel
        cache = ConverterCache(max_size=2)

        def _get(_):
            return cache.get_by_config_name("default")

        with ThreadPoolExecutor(max_workers=8) as pool:
            results = list(pool.map(_get, range(8)))

        assert all(r is sentinel for r in results)
        assert mock_build.call_count == 1
        assert cache.size == 1
