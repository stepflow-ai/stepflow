"""Tests for document chunking component."""

from __future__ import annotations

from unittest.mock import patch

import pytest

from docling_step_worker.chunk import chunk_document
from docling_step_worker.exceptions import ChunkingError


class TestChunkDocument:
    """Tests for async chunk_document."""

    @pytest.mark.asyncio
    @patch("docling_step_worker.chunk._run_chunking")
    async def test_reconstitutes_docling_document(
        self, mock_run_chunking, mock_context, sample_docling_document_dict
    ):
        mock_run_chunking.return_value = {
            "chunks": [{"text": "test chunk", "metadata": {}}],
            "chunk_count": 1,
        }

        result = await chunk_document(
            {"document": sample_docling_document_dict},
            mock_context,
        )

        mock_run_chunking.assert_called_once()
        call_args = mock_run_chunking.call_args[0]
        assert call_args[0] == sample_docling_document_dict

    @pytest.mark.asyncio
    @patch("docling_step_worker.chunk._run_chunking")
    async def test_returns_text_and_metadata(
        self, mock_run_chunking, mock_context, sample_docling_document_dict
    ):
        mock_run_chunking.return_value = {
            "chunks": [
                {"text": "chunk 1", "metadata": {"headings": ["Section 1"]}},
                {"text": "chunk 2", "metadata": {"page": [1]}},
            ],
            "chunk_count": 2,
        }

        result = await chunk_document(
            {"document": sample_docling_document_dict},
            mock_context,
        )

        assert isinstance(result["chunks"], list)
        assert len(result["chunks"]) == 2
        assert result["chunk_count"] == 2
        for chunk in result["chunks"]:
            assert "text" in chunk
            assert "metadata" in chunk

    @pytest.mark.asyncio
    @patch("docling_step_worker.chunk._run_chunking")
    async def test_applies_custom_options(
        self, mock_run_chunking, mock_context, sample_docling_document_dict
    ):
        mock_run_chunking.return_value = {"chunks": [], "chunk_count": 0}

        custom_opts = {"max_tokens": 256, "tokenizer": "custom/tokenizer"}
        await chunk_document(
            {
                "document": sample_docling_document_dict,
                "chunk_options": custom_opts,
            },
            mock_context,
        )

        call_args = mock_run_chunking.call_args[0]
        assert call_args[1] == custom_opts

    @pytest.mark.asyncio
    @patch("docling_step_worker.chunk._run_chunking")
    async def test_uses_default_options_when_none_provided(
        self, mock_run_chunking, mock_context, sample_docling_document_dict
    ):
        mock_run_chunking.return_value = {"chunks": [], "chunk_count": 0}

        await chunk_document(
            {"document": sample_docling_document_dict},
            mock_context,
        )

        call_args = mock_run_chunking.call_args[0]
        assert call_args[1] is None  # No chunk_options passed

    @pytest.mark.asyncio
    async def test_handles_empty_document(self, mock_context):
        """Empty document body should return empty chunks."""
        empty_doc = {
            "body": {"children": [], "self_ref": "#/body", "content_layer": "body"},
            "texts": [],
        }
        result = await chunk_document(
            {"document": empty_doc},
            mock_context,
        )
        assert result["chunks"] == []
        assert result["chunk_count"] == 0

    @pytest.mark.asyncio
    async def test_handles_none_document(self, mock_context):
        result = await chunk_document(
            {"document": None},
            mock_context,
        )
        assert result["chunks"] == []
        assert result["chunk_count"] == 0

    @pytest.mark.asyncio
    @patch("docling_step_worker.chunk.get_document_dict")
    @patch("docling_step_worker.chunk._run_chunking")
    async def test_handles_document_from_blob_ref(
        self,
        mock_run_chunking,
        mock_get_doc,
        mock_context,
        sample_docling_document_dict,
    ):
        mock_get_doc.return_value = sample_docling_document_dict
        mock_run_chunking.return_value = {
            "chunks": [{"text": "test", "metadata": {}}],
            "chunk_count": 1,
        }

        result = await chunk_document(
            {"document": "sha256:abc123"},
            mock_context,
        )

        mock_get_doc.assert_awaited_once_with("sha256:abc123")
        assert result["chunk_count"] == 1

    @pytest.mark.asyncio
    @patch("docling_step_worker.chunk.get_document_dict")
    async def test_handles_blob_ref_error(self, mock_get_doc, mock_context):
        mock_get_doc.side_effect = RuntimeError("blob not found")

        with pytest.raises(ChunkingError, match="Failed to retrieve document"):
            await chunk_document(
                {"document": "sha256:missing"},
                mock_context,
            )

    @pytest.mark.asyncio
    @patch("docling_step_worker.chunk._run_chunking")
    async def test_handles_chunking_error(
        self, mock_run_chunking, mock_context, sample_docling_document_dict
    ):
        mock_run_chunking.side_effect = RuntimeError("chunker crashed")

        with pytest.raises(ChunkingError, match="Chunking failed"):
            await chunk_document(
                {"document": sample_docling_document_dict},
                mock_context,
            )

    @pytest.mark.asyncio
    async def test_rejects_non_dict_document(self, mock_context):
        with pytest.raises(ChunkingError, match="Expected document dict"):
            await chunk_document(
                {"document": 12345},
                mock_context,
            )
