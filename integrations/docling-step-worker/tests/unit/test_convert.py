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

"""Tests for document conversion component.

These tests mock the DocumentConverter to avoid loading real models in unit tests.
"""

from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest

from docling_step_worker.convert import _run_conversion, convert_document


def _make_mock_doc():
    """Create a mock DoclingDocument with export methods."""
    mock_doc = MagicMock()
    mock_doc.export_to_dict.return_value = {
        "schema_name": "DoclingDocument",
        "name": "test",
        "pages": {"1": {}},
        "tables": [],
    }
    mock_doc.export_to_markdown.return_value = "# Test\n\nBody"
    mock_doc.export_to_html.return_value = "<h1>Test</h1>"
    mock_doc.export_to_text.return_value = "Test\nBody"
    mock_doc.export_to_doctags.return_value = "<doctag>content</doctag>"
    return mock_doc


def _make_mock_converter(mock_doc=None):
    """Create a mock DocumentConverter that returns a mock result."""
    if mock_doc is None:
        mock_doc = _make_mock_doc()
    mock_result = MagicMock()
    mock_result.document = mock_doc
    mock_converter = MagicMock()
    mock_converter.convert.return_value = mock_result
    return mock_converter


class TestRunConversion:
    """Tests for synchronous conversion helper."""

    def test_calls_converter_with_document_stream(self):
        mock_converter = _make_mock_converter()

        _run_conversion(mock_converter, b"fake pdf", "test.pdf", to_formats=["json"])

        # Verify converter was called with a DocumentStream
        call_args = mock_converter.convert.call_args
        source = call_args.kwargs.get("source") or call_args[1].get("source")
        from docling.datamodel.base_models import DocumentStream

        assert isinstance(source, DocumentStream)
        assert source.name == "test.pdf"

    def test_returns_export_document_response(self):
        mock_converter = _make_mock_converter()

        result = _run_conversion(
            mock_converter, b"fake pdf", "doc.pdf", to_formats=["json"]
        )

        assert result["status"] == "success"
        assert isinstance(result["document"], dict)
        assert result["document"]["filename"] == "doc.pdf"
        assert result["document"]["json_content"]["schema_name"] == "DoclingDocument"
        assert isinstance(result["processing_time"], float)
        assert result["processing_time"] >= 0
        assert isinstance(result["errors"], list)
        assert isinstance(result["timings"], dict)

    def test_document_dict_always_present(self):
        mock_converter = _make_mock_converter()

        result = _run_conversion(
            mock_converter, b"fake pdf", "doc.pdf", to_formats=["markdown"]
        )

        assert "document_dict" in result
        assert result["document_dict"]["schema_name"] == "DoclingDocument"

    def test_no_legacy_fields(self):
        mock_converter = _make_mock_converter()

        result = _run_conversion(
            mock_converter, b"fake pdf", "doc.pdf", to_formats=["json"]
        )

        assert "page_count" not in result
        assert "table_count" not in result
        assert "processing_time_ms" not in result

    def test_markdown_content_with_default_formats(self):
        mock_converter = _make_mock_converter()

        result = _run_conversion(mock_converter, b"fake pdf", "doc.pdf")

        assert result["document"]["md_content"] == "# Test\n\nBody"
        assert "json_content" not in result["document"]


class TestConvertDocument:
    """Tests for async convert_document."""

    @pytest.mark.asyncio
    @patch("docling_step_worker.convert.put_document_blob")
    @patch("docling_step_worker.convert.get_document_bytes")
    async def test_calls_converter_with_document_stream(
        self, mock_get_bytes, mock_put_blob, mock_context
    ):
        mock_get_bytes.return_value = b"fake pdf bytes"
        mock_put_blob.return_value = "sha256:doc123"
        mock_converter = _make_mock_converter()

        result = await convert_document(
            {
                "source": "blob:sha256:abc",
                "source_kind": "blob",
                "to_formats": ["json"],
            },
            mock_context,
            mock_converter,
        )

        assert result["status"] == "success"
        assert result["document"]["filename"] == "document.pdf"
        mock_get_bytes.assert_awaited_once()

    @pytest.mark.asyncio
    @patch("docling_step_worker.convert.put_document_blob")
    @patch("docling_step_worker.convert.get_document_bytes")
    async def test_returns_docling_serve_response_shape(
        self, mock_get_bytes, mock_put_blob, mock_context
    ):
        mock_get_bytes.return_value = b"pdf bytes"
        mock_put_blob.return_value = "sha256:doc123"
        mock_converter = _make_mock_converter()

        result = await convert_document(
            {
                "source": "blob:sha256:abc",
                "source_kind": "blob",
                "to_formats": ["markdown"],
            },
            mock_context,
            mock_converter,
        )

        assert result["status"] == "success"
        assert isinstance(result["document"], dict)
        assert isinstance(result["processing_time"], float)
        assert isinstance(result["errors"], list)
        assert isinstance(result["timings"], dict)
        assert "document_dict" in result
        # Legacy fields should not be present
        assert "page_count" not in result
        assert "table_count" not in result
        assert "processing_time_ms" not in result

    @pytest.mark.asyncio
    @patch("docling_step_worker.convert.get_document_bytes")
    async def test_does_not_accept_pipeline_options(self, mock_get_bytes, mock_context):
        """Pipeline options in input should be ignored."""
        mock_get_bytes.return_value = b"pdf bytes"
        mock_converter = _make_mock_converter()

        # pipeline_options in input should be ignored
        await convert_document(
            {
                "source": "blob:sha256:abc",
                "source_kind": "blob",
                "pipeline_options": {"do_ocr": True},
            },
            mock_context,
            mock_converter,
        )

        # converter.convert() should NOT receive pipeline_options
        call_kwargs = mock_converter.convert.call_args.kwargs
        assert "pipeline_options" not in call_kwargs

    @pytest.mark.asyncio
    @patch("docling_step_worker.convert.put_document_blob")
    @patch("docling_step_worker.convert.get_document_bytes")
    async def test_stores_document_dict_in_blob_store(
        self, mock_get_bytes, mock_put_blob, mock_context
    ):
        mock_get_bytes.return_value = b"pdf bytes"
        mock_put_blob.return_value = "sha256:stored_doc"
        mock_converter = _make_mock_converter()

        doc_dict = {
            "schema_name": "DoclingDocument",
            "name": "test",
            "pages": {"1": {}},
            "tables": [],
        }

        result = await convert_document(
            {"source": "blob:sha256:abc", "source_kind": "blob"},
            mock_context,
            mock_converter,
        )

        # Blob store receives document_dict, not the full response
        mock_put_blob.assert_awaited_once_with(doc_dict)
        assert result["document_blob_ref"] == "sha256:stored_doc"

    @pytest.mark.asyncio
    @patch("docling_step_worker.convert.get_document_bytes")
    async def test_handles_conversion_error(self, mock_get_bytes, mock_context):
        mock_get_bytes.return_value = b"pdf bytes"

        mock_converter = MagicMock()
        mock_converter.convert.side_effect = RuntimeError("conversion failed")

        result = await convert_document(
            {"source": "blob:sha256:abc", "source_kind": "blob"},
            mock_context,
            mock_converter,
        )

        assert result["status"] == "failure"
        assert isinstance(result["errors"], list)
        assert len(result["errors"]) == 1
        assert "conversion failed" in result["errors"][0]["error_message"]
        assert result["document"] is None
        assert result["document_dict"] is None

    @pytest.mark.asyncio
    @patch("docling_step_worker.convert.get_document_bytes")
    async def test_handles_retrieval_error(self, mock_get_bytes, mock_context):
        mock_get_bytes.side_effect = RuntimeError("blob not found")

        mock_converter = MagicMock()

        result = await convert_document(
            {"source": "blob:sha256:missing", "source_kind": "blob"},
            mock_context,
            mock_converter,
        )

        assert result["status"] == "failure"
        assert isinstance(result["errors"], list)
        assert len(result["errors"]) == 1
        assert "blob not found" in result["errors"][0]["error_message"]
        assert result["document"] is None
        assert result["document_dict"] is None
        mock_converter.convert.assert_not_called()

    @pytest.mark.asyncio
    @patch("docling_step_worker.convert.put_document_blob")
    @patch("docling_step_worker.convert.get_document_bytes")
    async def test_to_formats_param_passed_through(
        self, mock_get_bytes, mock_put_blob, mock_context
    ):
        mock_get_bytes.return_value = b"pdf bytes"
        mock_put_blob.return_value = "sha256:doc"
        mock_converter = _make_mock_converter()

        result = await convert_document(
            {
                "source": "blob:sha256:abc",
                "source_kind": "blob",
                "to_formats": ["markdown", "json"],
            },
            mock_context,
            mock_converter,
        )

        assert "md_content" in result["document"]
        assert "json_content" in result["document"]
