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

"""Tests for blob_ref module: BlobRef, blobify_inputs, resolve_blob_refs."""

import hashlib
import json
from unittest.mock import AsyncMock, MagicMock

import pytest

from stepflow_py.api.models.blob_type import BlobType
from stepflow_py.worker import blob_store
from stepflow_py.worker.blob_ref import (
    BlobRef,
    blobify_inputs,
    is_blob_ref,
    resolve_blob_refs,
)

# ---------------------------------------------------------------------------
# BlobRef unit tests
# ---------------------------------------------------------------------------


class TestBlobRef:
    def test_to_dict_data_type(self):
        ref = BlobRef(blob_id="a" * 64, blob_type=BlobType.DATA, size=100)
        d = ref.to_dict()
        assert d["$blob"] == "a" * 64
        assert "blobType" not in d  # DATA is the default, omitted
        assert d["size"] == 100

    def test_to_dict_binary_type(self):
        ref = BlobRef(blob_id="b" * 64, blob_type=BlobType.BINARY, size=200)
        d = ref.to_dict()
        assert d["$blob"] == "b" * 64
        assert d["blobType"] == "binary"
        assert d["size"] == 200

    def test_from_dict_valid(self):
        d = {"$blob": "c" * 64, "blobType": "data", "size": 50}
        ref = BlobRef.from_dict(d)
        assert ref is not None
        assert ref.blob_id == "c" * 64
        assert ref.blob_type == BlobType.DATA
        assert ref.size == 50

    def test_from_dict_missing_blob_key(self):
        assert BlobRef.from_dict({"notABlob": True}) is None

    def test_from_dict_wrong_id_length(self):
        assert BlobRef.from_dict({"$blob": "tooshort"}) is None

    def test_is_blob_ref(self):
        assert is_blob_ref({"$blob": "a" * 64}) is True
        assert is_blob_ref({"key": "value"}) is False
        assert is_blob_ref("not a dict") is False
        assert is_blob_ref(42) is False


# ---------------------------------------------------------------------------
# Mock blob store helpers
# ---------------------------------------------------------------------------


class MockBlobStore:
    """In-memory blob store for testing blobify/resolve round-trips."""

    def __init__(self):
        self._blobs: dict[str, any] = {}
        self._binary_blobs: dict[str, bytes] = {}

    async def put(self, data, blob_type="data"):
        content_str = json.dumps(data, sort_keys=True)
        blob_id = hashlib.sha256(content_str.encode()).hexdigest()
        self._blobs[blob_id] = data
        return blob_id

    async def put_binary(self, data: bytes):
        blob_id = hashlib.sha256(data).hexdigest()
        self._binary_blobs[blob_id] = data
        return blob_id

    async def get(self, blob_id):
        return self._blobs[blob_id]

    def get_binary(self, blob_id: str) -> bytes:
        return self._binary_blobs[blob_id]


@pytest.fixture
def mock_blob_store():
    """Set up a mock blob store via contextvars for testing."""
    store = MockBlobStore()

    # Create a mock httpx client that delegates to our in-memory store.
    # Supports both JSON and octet-stream content types.
    mock_client = AsyncMock()

    async def mock_post(url, *, json=None, content=None, headers=None):
        headers = headers or {}
        ct = headers.get("content-type", "application/json")
        resp = MagicMock()
        resp.raise_for_status = MagicMock()

        if ct.startswith("application/octet-stream"):
            blob_id = await store.put_binary(content)
        else:
            data = json["data"]
            blob_id = await store.put(data, json.get("blobType", "data"))

        resp.json.return_value = {"blobId": blob_id}
        return resp

    async def mock_get(url, *, headers=None):
        headers = headers or {}
        accept = headers.get("accept", "application/json")
        blob_id = url.rsplit("/", 1)[-1]
        resp = MagicMock()
        resp.raise_for_status = MagicMock()

        if accept.startswith("application/octet-stream"):
            resp.content = store.get_binary(blob_id)
        else:
            data = await store.get(blob_id)
            resp.json.return_value = {"data": data}

        return resp

    mock_client.post = mock_post
    mock_client.get = mock_get

    tokens = blob_store.configure(
        blob_api_url="http://test-blob-api/blobs",
        http_client=mock_client,
    )
    yield store
    blob_store.reset(tokens)


# ---------------------------------------------------------------------------
# blobify_inputs tests
# ---------------------------------------------------------------------------


class TestBlobifyInputs:
    @pytest.mark.asyncio
    async def test_below_threshold(self, mock_blob_store):
        """Fields below threshold are not blobified."""
        input_data = {"small": "hello"}
        result, ids = await blobify_inputs(input_data, 1000)
        assert result == input_data
        assert ids == []

    @pytest.mark.asyncio
    async def test_above_threshold(self, mock_blob_store):
        """Fields above threshold are replaced with blob refs."""
        large_value = "x" * 200
        input_data = {"large": large_value, "small": "ok"}
        result, ids = await blobify_inputs(input_data, 100)
        assert len(ids) == 1
        assert is_blob_ref(result["large"])
        assert result["small"] == "ok"
        # Verify the blob ref has correct metadata
        ref = BlobRef.from_dict(result["large"])
        assert ref is not None
        assert ref.blob_type == BlobType.DATA
        assert ref.size > 100

    @pytest.mark.asyncio
    async def test_zero_threshold_disabled(self, mock_blob_store):
        """Threshold of 0 means blobification is disabled."""
        input_data = {"data": "x" * 1000}
        result, ids = await blobify_inputs(input_data, 0)
        assert result == input_data
        assert ids == []

    @pytest.mark.asyncio
    async def test_non_dict_passthrough(self, mock_blob_store):
        """Non-dict values pass through unchanged."""
        result, ids = await blobify_inputs("just a string", 1)
        assert result == "just a string"
        assert ids == []

    @pytest.mark.asyncio
    async def test_existing_blob_ref_not_double_blobified(self, mock_blob_store):
        """Existing blob refs in input are not re-blobified."""
        existing_ref = {"$blob": "a" * 64, "blobType": "data"}
        input_data = {"already_blob": existing_ref, "normal": "value"}
        result, ids = await blobify_inputs(input_data, 1000)
        assert result["already_blob"] == existing_ref
        assert ids == []  # No new blobs created (normal field is below 1000 bytes)


# ---------------------------------------------------------------------------
# resolve_blob_refs tests
# ---------------------------------------------------------------------------


class TestResolveBlobRefs:
    @pytest.mark.asyncio
    async def test_resolve_simple(self, mock_blob_store):
        """Blob refs are resolved to their inline data."""
        # Store a value
        blob_id = await mock_blob_store.put({"resolved": "data"})

        # Create input with a blob ref
        input_data = {"field": {"$blob": blob_id}}
        result = await resolve_blob_refs(input_data)
        assert result == {"field": {"resolved": "data"}}

    @pytest.mark.asyncio
    async def test_resolve_in_array(self, mock_blob_store):
        """Blob refs inside arrays are resolved."""
        blob_id = await mock_blob_store.put("hello")
        input_data = [{"$blob": blob_id}, "plain"]
        result = await resolve_blob_refs(input_data)
        assert result == ["hello", "plain"]

    @pytest.mark.asyncio
    async def test_no_blob_refs_passthrough(self, mock_blob_store):
        """Values without blob refs pass through unchanged."""
        input_data = {"normal": "data", "nested": {"key": "value"}}
        result = await resolve_blob_refs(input_data)
        assert result == input_data

    @pytest.mark.asyncio
    async def test_round_trip(self, mock_blob_store):
        """Blobify then resolve should produce the original value."""
        large_value = "x" * 200
        input_data = {"large": large_value, "small": "ok"}

        blobified, ids = await blobify_inputs(input_data, 100)
        assert len(ids) == 1
        assert blobified != input_data

        resolved = await resolve_blob_refs(blobified)
        assert resolved == input_data

    @pytest.mark.asyncio
    async def test_resolve_multiple_parallel(self, mock_blob_store):
        """Multiple blob refs in one value are resolved in parallel."""
        blob_id1 = await mock_blob_store.put("first")
        blob_id2 = await mock_blob_store.put("second")
        blob_id3 = await mock_blob_store.put("third")

        input_data = {
            "a": {"$blob": blob_id1},
            "b": {"$blob": blob_id2},
            "c": [{"$blob": blob_id3}, "plain"],
        }
        result = await resolve_blob_refs(input_data)
        assert result == {"a": "first", "b": "second", "c": ["third", "plain"]}


# ---------------------------------------------------------------------------
# Binary blob store function tests (put_blob_binary / get_blob_binary)
# ---------------------------------------------------------------------------


class TestBinaryBlobStore:
    @pytest.mark.asyncio
    async def test_put_get_binary_round_trip(self, mock_blob_store):
        """Binary data round-trips through put/get via octet-stream."""
        data = b"Hello, binary world! \x00\x01\x02\xff"
        blob_id = await blob_store.put_blob_binary(data)
        assert len(blob_id) == 64  # SHA-256 hex
        result = await blob_store.get_blob_binary(blob_id)
        assert result == data

    @pytest.mark.asyncio
    async def test_put_binary_with_filename(self, mock_blob_store):
        """put_blob_binary accepts an optional filename parameter."""
        data = b"file content"
        blob_id = await blob_store.put_blob_binary(data, filename="test.pdf")
        assert len(blob_id) == 64

    @pytest.mark.asyncio
    async def test_put_binary_deduplication(self, mock_blob_store):
        """Same binary data produces the same blob ID."""
        data = b"dedup test data"
        id1 = await blob_store.put_blob_binary(data)
        id2 = await blob_store.put_blob_binary(data)
        assert id1 == id2
