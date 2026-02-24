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

"""Tests for blob store and document source helpers."""

from __future__ import annotations

from unittest.mock import AsyncMock, patch

import pytest

from docling_step_worker.blob_utils import (
    bytes_to_document_stream,
    get_document_bytes,
    get_document_dict,
    put_document_blob,
)
from docling_step_worker.exceptions import BlobStoreError


class TestBytesToDocumentStream:
    def test_wraps_correctly(self):
        data = b"fake pdf content"
        stream = bytes_to_document_stream(data, "test.pdf")
        assert stream.name == "test.pdf"
        assert stream.stream.read() == b"fake pdf content"

    def test_default_filename(self):
        data = b"content"
        stream = bytes_to_document_stream(data)
        assert stream.name == "document.pdf"

    def test_stream_is_seekable(self):
        data = b"hello world"
        stream = bytes_to_document_stream(data, "doc.pdf")
        # Read once
        stream.stream.read()
        # Seek back and read again
        stream.stream.seek(0)
        assert stream.stream.read() == b"hello world"


class TestGetDocumentBytes:
    @pytest.mark.asyncio
    @patch("docling_step_worker.blob_utils.blob_store")
    async def test_from_blob(self, mock_blob_store):
        mock_blob_store.get_blob_binary = AsyncMock(return_value=b"pdf bytes")
        result = await get_document_bytes("sha256:abc123", "blob")
        assert result == b"pdf bytes"
        mock_blob_store.get_blob_binary.assert_awaited_once_with("sha256:abc123")

    @pytest.mark.asyncio
    @patch("docling_step_worker.blob_utils.blob_store")
    async def test_from_blob_error(self, mock_blob_store):
        mock_blob_store.get_blob_binary = AsyncMock(
            side_effect=RuntimeError("not found")
        )
        with pytest.raises(BlobStoreError, match="Failed to retrieve blob"):
            await get_document_bytes("sha256:bad", "blob")

    @pytest.mark.asyncio
    @patch("docling_step_worker.blob_utils.httpx.AsyncClient")
    async def test_from_url(self, mock_client_cls):
        mock_response = AsyncMock()
        mock_response.content = b"downloaded pdf"
        mock_response.raise_for_status = lambda: None

        mock_client = AsyncMock()
        mock_client.get = AsyncMock(return_value=mock_response)
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=None)
        mock_client_cls.return_value = mock_client

        result = await get_document_bytes("https://example.com/doc.pdf", "url")
        assert result == b"downloaded pdf"

    @pytest.mark.asyncio
    async def test_unknown_source_kind(self):
        with pytest.raises(BlobStoreError, match="Unknown source_kind"):
            await get_document_bytes("something", "ftp")


class TestPutDocumentBlob:
    @pytest.mark.asyncio
    @patch("docling_step_worker.blob_utils.blob_store")
    async def test_stores_dict(self, mock_blob_store):
        mock_blob_store.put_blob = AsyncMock(return_value="sha256:abc123")
        doc_dict = {"schema_name": "DoclingDocument", "name": "test"}
        result = await put_document_blob(doc_dict)
        assert result == "sha256:abc123"
        mock_blob_store.put_blob.assert_awaited_once_with(doc_dict)

    @pytest.mark.asyncio
    @patch("docling_step_worker.blob_utils.blob_store")
    async def test_stores_error(self, mock_blob_store):
        mock_blob_store.put_blob = AsyncMock(side_effect=RuntimeError("fail"))
        with pytest.raises(BlobStoreError, match="Failed to store document blob"):
            await put_document_blob({"data": "test"})


class TestGetDocumentDict:
    @pytest.mark.asyncio
    @patch("docling_step_worker.blob_utils.blob_store")
    async def test_retrieves_from_blob(self, mock_blob_store):
        expected = {"schema_name": "DoclingDocument", "name": "test"}
        mock_blob_store.get_blob = AsyncMock(return_value=expected)
        result = await get_document_dict("sha256:abc123")
        assert result == expected
        mock_blob_store.get_blob.assert_awaited_once_with("sha256:abc123")

    @pytest.mark.asyncio
    @patch("docling_step_worker.blob_utils.blob_store")
    async def test_retrieves_non_dict_error(self, mock_blob_store):
        mock_blob_store.get_blob = AsyncMock(return_value="not a dict")
        with pytest.raises(BlobStoreError, match="Expected dict"):
            await get_document_dict("sha256:abc123")

    @pytest.mark.asyncio
    @patch("docling_step_worker.blob_utils.blob_store")
    async def test_retrieves_error(self, mock_blob_store):
        mock_blob_store.get_blob = AsyncMock(side_effect=RuntimeError("fail"))
        with pytest.raises(BlobStoreError, match="Failed to retrieve document blob"):
            await get_document_dict("sha256:abc123")
