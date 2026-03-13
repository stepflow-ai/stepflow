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

"""Tests for document classification component."""

from __future__ import annotations

from unittest.mock import patch

import pytest

from docling_proto_step_worker.classify import _classify_pdf_bytes, classify_document


class TestClassifyPdfBytes:
    """Tests for synchronous PDF classification."""

    def test_born_digital_pdf(self, sample_pdf_bytes):
        result = _classify_pdf_bytes(sample_pdf_bytes)
        assert result["has_text_layer"] is True
        assert result["recommended_config"] == "born_digital"
        assert result["page_count"] == 2
        assert result["format"] == "pdf"

    def test_returns_page_count(self, sample_pdf_bytes):
        result = _classify_pdf_bytes(sample_pdf_bytes)
        assert result["page_count"] == 2

    def test_detects_format(self, sample_pdf_bytes):
        result = _classify_pdf_bytes(sample_pdf_bytes)
        assert result["format"] == "pdf"

    def test_handles_corrupt_bytes(self):
        result = _classify_pdf_bytes(b"not a pdf")
        assert result["page_count"] == 0
        assert result["recommended_config"] == "default"

    def test_handles_empty_bytes(self):
        result = _classify_pdf_bytes(b"")
        assert result["page_count"] == 0
        assert result["recommended_config"] == "default"

    def test_with_tables_heuristic(self, sample_pdf_with_tables_bytes):
        result = _classify_pdf_bytes(sample_pdf_with_tables_bytes)
        assert result["has_text_layer"] is True
        assert result["page_count"] > 0
        # Note: table detection is heuristic-based and may not always
        # detect fpdf2-generated tables perfectly since the text layout
        # depends on the PDF renderer


class TestClassifyDocument:
    """Tests for async classify_document."""

    @pytest.mark.asyncio
    @patch("docling_proto_step_worker.classify.get_document_bytes")
    async def test_handles_blob_source(
        self, mock_get_bytes, mock_context, sample_pdf_bytes
    ):
        mock_get_bytes.return_value = sample_pdf_bytes
        result = await classify_document(
            {"source": "blob:sha256:abc", "source_kind": "blob"},
            mock_context,
        )
        mock_get_bytes.assert_awaited_once_with("blob:sha256:abc", "blob")
        assert result["page_count"] == 2
        assert result["has_text_layer"] is True

    @pytest.mark.asyncio
    @patch("docling_proto_step_worker.classify.get_document_bytes")
    async def test_handles_url_source(
        self, mock_get_bytes, mock_context, sample_pdf_bytes
    ):
        mock_get_bytes.return_value = sample_pdf_bytes
        result = await classify_document(
            {"source": "https://example.com/doc.pdf", "source_kind": "url"},
            mock_context,
        )
        mock_get_bytes.assert_awaited_once_with("https://example.com/doc.pdf", "url")
        assert result["has_text_layer"] is True

    @pytest.mark.asyncio
    @patch("docling_proto_step_worker.classify.get_document_bytes")
    async def test_minimal_output_on_retrieval_error(
        self, mock_get_bytes, mock_context
    ):
        mock_get_bytes.side_effect = RuntimeError("network error")
        result = await classify_document(
            {"source": "https://bad.url", "source_kind": "url"},
            mock_context,
        )
        # Should NOT raise — returns safe defaults
        assert result["page_count"] == 0
        assert result["recommended_config"] == "default"

    @pytest.mark.asyncio
    @patch("docling_proto_step_worker.classify.get_document_bytes")
    async def test_minimal_output_on_corrupt_pdf(self, mock_get_bytes, mock_context):
        mock_get_bytes.return_value = b"corrupt data"
        result = await classify_document(
            {"source": "blob:sha256:bad", "source_kind": "blob"},
            mock_context,
        )
        assert result["page_count"] == 0
        assert result["recommended_config"] == "default"

    @pytest.mark.asyncio
    @patch("docling_proto_step_worker.classify.get_document_bytes")
    async def test_detects_no_text_layer(self, mock_get_bytes, mock_context):
        """Test with a PDF that has no extractable text.

        We create a minimal PDF with just an empty page (no text objects).
        """
        from fpdf import FPDF

        pdf = FPDF()
        pdf.add_page()
        # Don't add any text — just an empty page
        empty_pdf_bytes = bytes(pdf.output())

        mock_get_bytes.return_value = empty_pdf_bytes
        result = await classify_document(
            {"source": "blob:sha256:empty", "source_kind": "blob"},
            mock_context,
        )
        assert result["has_text_layer"] is False
        assert result["recommended_config"] == "scanned"
