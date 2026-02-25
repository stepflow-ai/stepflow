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


class TestRunConversion:
    """Tests for synchronous conversion helper."""

    def test_calls_converter_with_document_stream(self):
        # Mock converter and result
        mock_doc = MagicMock()
        mock_doc.export_to_dict.return_value = {
            "schema_name": "DoclingDocument",
            "name": "test",
            "pages": {"1": {}},
            "tables": [],
        }

        mock_result = MagicMock()
        mock_result.document = mock_doc

        mock_converter = MagicMock()
        mock_converter.convert.return_value = mock_result

        result = _run_conversion(mock_converter, b"fake pdf", "test.pdf")

        # Verify converter was called with a DocumentStream
        call_args = mock_converter.convert.call_args
        source = call_args.kwargs.get("source") or call_args[1].get("source")
        from docling.datamodel.base_models import DocumentStream

        assert isinstance(source, DocumentStream)
        assert source.name == "test.pdf"

    def test_returns_docling_document_dict(self):
        mock_doc = MagicMock()
        mock_doc.export_to_dict.return_value = {
            "schema_name": "DoclingDocument",
            "name": "test",
            "pages": {"1": {}, "2": {}},
            "tables": [{"id": "t1"}, {"id": "t2"}],
        }

        mock_result = MagicMock()
        mock_result.document = mock_doc

        mock_converter = MagicMock()
        mock_converter.convert.return_value = mock_result

        result = _run_conversion(mock_converter, b"fake pdf", "doc.pdf")

        assert result["status"] == "success"
        assert isinstance(result["document"], dict)
        assert result["document"]["schema_name"] == "DoclingDocument"
        assert result["page_count"] == 2
        assert result["table_count"] == 2
        assert result["processing_time_ms"] >= 0

    def test_includes_table_count_in_output(self):
        mock_doc = MagicMock()
        mock_doc.export_to_dict.return_value = {
            "pages": {},
            "tables": [{"id": "t1"}, {"id": "t2"}, {"id": "t3"}],
        }

        mock_result = MagicMock()
        mock_result.document = mock_doc

        mock_converter = MagicMock()
        mock_converter.convert.return_value = mock_result

        result = _run_conversion(mock_converter, b"pdf", "doc.pdf")
        assert result["table_count"] == 3


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

        mock_doc = MagicMock()
        mock_doc.export_to_dict.return_value = {
            "schema_name": "DoclingDocument",
            "pages": {"1": {}},
            "tables": [],
        }
        mock_result = MagicMock()
        mock_result.document = mock_doc

        mock_converter = MagicMock()
        mock_converter.convert.return_value = mock_result

        result = await convert_document(
            {"source": "blob:sha256:abc", "source_kind": "blob"},
            mock_context,
            mock_converter,
        )

        assert result["status"] == "success"
        assert result["document"]["schema_name"] == "DoclingDocument"
        mock_get_bytes.assert_awaited_once()

    @pytest.mark.asyncio
    @patch("docling_step_worker.convert.put_document_blob")
    @patch("docling_step_worker.convert.get_document_bytes")
    async def test_returns_docling_document_dict(
        self, mock_get_bytes, mock_put_blob, mock_context
    ):
        mock_get_bytes.return_value = b"pdf bytes"
        mock_put_blob.return_value = "sha256:doc123"

        mock_doc = MagicMock()
        mock_doc.export_to_dict.return_value = {
            "schema_name": "DoclingDocument",
            "name": "test",
            "pages": {"1": {}},
            "tables": [],
        }
        mock_result = MagicMock()
        mock_result.document = mock_doc

        mock_converter = MagicMock()
        mock_converter.convert.return_value = mock_result

        result = await convert_document(
            {"source": "blob:sha256:abc", "source_kind": "blob"},
            mock_context,
            mock_converter,
        )

        assert result["status"] == "success"
        assert isinstance(result["document"], dict)
        assert "page_count" in result
        assert "processing_time_ms" in result

    @pytest.mark.asyncio
    @patch("docling_step_worker.convert.get_document_bytes")
    async def test_does_not_accept_pipeline_options(self, mock_get_bytes, mock_context):
        """Pipeline options in input should be ignored."""
        mock_get_bytes.return_value = b"pdf bytes"

        mock_doc = MagicMock()
        mock_doc.export_to_dict.return_value = {
            "pages": {},
            "tables": [],
        }
        mock_result = MagicMock()
        mock_result.document = mock_doc

        mock_converter = MagicMock()
        mock_converter.convert.return_value = mock_result

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
    async def test_stores_document_in_blob_store(
        self, mock_get_bytes, mock_put_blob, mock_context
    ):
        mock_get_bytes.return_value = b"pdf bytes"
        mock_put_blob.return_value = "sha256:stored_doc"

        mock_doc = MagicMock()
        doc_dict = {"schema_name": "DoclingDocument", "pages": {}, "tables": []}
        mock_doc.export_to_dict.return_value = doc_dict
        mock_result = MagicMock()
        mock_result.document = mock_doc

        mock_converter = MagicMock()
        mock_converter.convert.return_value = mock_result

        result = await convert_document(
            {"source": "blob:sha256:abc", "source_kind": "blob"},
            mock_context,
            mock_converter,
        )

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

        assert result["status"] == "error"
        assert "error" in result

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

        assert result["status"] == "error"
        assert "error" in result
        mock_converter.convert.assert_not_called()
