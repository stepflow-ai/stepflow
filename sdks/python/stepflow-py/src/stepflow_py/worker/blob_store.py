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

"""Module-level blob store API using gRPC BlobService.

This module provides blob API functions that can be used by any code
without requiring a StepflowContext parameter.  The blob service URL
is read from the ``STEPFLOW_BLOB_URL`` environment variable at module
import time.

Usage::

    from stepflow_py.worker import blob_store

    # In any async code (component, executor, framework):
    blob_id = await blob_store.put_blob({"key": "value"})
    data = await blob_store.get_blob(blob_id)
"""

from __future__ import annotations

import logging
import os
from typing import Any

import grpc
import grpc.aio

from stepflow_py.worker.blob_ref import BLOB_TYPE_DATA

logger = logging.getLogger(__name__)

# Static configuration from environment variable.
# Read once at module import; does not change during worker lifetime.
_BLOB_URL: str = os.environ.get("STEPFLOW_BLOB_URL", "")


def _get_blob_url() -> str:
    """Return the blob service URL or raise if not configured."""
    if not _BLOB_URL:
        raise RuntimeError(
            "Blob service URL not configured. "
            "Set the STEPFLOW_BLOB_URL environment variable."
        )
    return _BLOB_URL


async def put_blob(data: Any, blob_type: str = BLOB_TYPE_DATA) -> str:
    """Store JSON data as a blob and return its content-based ID (SHA-256).

    Args:
        data: The JSON-serializable data to store.
        blob_type: The type of blob to store ("flow" or "data").

    Returns:
        The blob ID (SHA-256 hash) for the stored data.

    Raises:
        RuntimeError: If blob service URL is not configured.
    """
    from stepflow_py.proto import PutBlobRequest
    from stepflow_py.proto.blobs_pb2_grpc import BlobServiceStub
    from stepflow_py.worker.observability import get_tracer
    from stepflow_py.worker.task_handler import python_to_proto_value

    url = _get_blob_url()
    tracer = get_tracer(__name__)
    with tracer.start_as_current_span(
        "put_blob",
        attributes={"blob_type": blob_type},
    ):
        json_data = python_to_proto_value(data)
        request = PutBlobRequest(
            json_data=json_data,
            blob_type=blob_type,
        )
        channel = grpc.aio.insecure_channel(url)
        try:
            stub = BlobServiceStub(channel)
            response = await stub.PutBlob(request)
            return response.blob_id
        finally:
            await channel.close()


async def get_blob(blob_id: str) -> Any:
    """Retrieve JSON data by blob ID.

    Args:
        blob_id: The blob ID to retrieve.

    Returns:
        The JSON data associated with the blob ID.

    Raises:
        RuntimeError: If blob service URL is not configured.
    """
    from stepflow_py.proto import GetBlobRequest
    from stepflow_py.proto.blobs_pb2_grpc import BlobServiceStub
    from stepflow_py.worker.observability import get_tracer
    from stepflow_py.worker.task_handler import proto_value_to_python

    url = _get_blob_url()
    tracer = get_tracer(__name__)
    with tracer.start_as_current_span(
        "get_blob",
        attributes={"blob_id": blob_id},
    ):
        request = GetBlobRequest(blob_id=blob_id)
        channel = grpc.aio.insecure_channel(url)
        try:
            stub = BlobServiceStub(channel)
            response = await stub.GetBlob(request)
            if response.HasField("json_data"):
                return proto_value_to_python(response.json_data)
            elif response.HasField("raw_data"):
                return response.raw_data
            else:
                return None
        finally:
            await channel.close()


async def get_blobs(blob_ids: set[str]) -> dict[str, Any]:
    """Retrieve multiple blobs in parallel.

    Fetches all blob IDs concurrently using ``asyncio.gather``.

    Args:
        blob_ids: Set of blob IDs to retrieve.

    Returns:
        A mapping from blob ID to its JSON data.

    Raises:
        RuntimeError: If blob service URL is not configured.
    """
    import asyncio

    if not blob_ids:
        return {}

    ids = list(blob_ids)
    results = await asyncio.gather(*(get_blob(bid) for bid in ids))
    return dict(zip(ids, results, strict=True))


async def put_blob_binary(data: bytes, *, filename: str | None = None) -> str:
    """Store raw binary data as a blob and return its content-based ID.

    Args:
        data: The raw bytes to store.
        filename: Optional filename metadata (currently unused by gRPC API).

    Returns:
        The blob ID (SHA-256 hash) for the stored data.

    Raises:
        RuntimeError: If blob service URL is not configured.
    """
    from stepflow_py.proto import PutBlobRequest
    from stepflow_py.proto.blobs_pb2_grpc import BlobServiceStub
    from stepflow_py.worker.observability import get_tracer

    url = _get_blob_url()
    tracer = get_tracer(__name__)
    with tracer.start_as_current_span(
        "put_blob_binary",
        attributes={"size": len(data)},
    ):
        request = PutBlobRequest(
            raw_data=bytes(data),
            blob_type="binary",
        )
        channel = grpc.aio.insecure_channel(url)
        try:
            stub = BlobServiceStub(channel)
            response = await stub.PutBlob(request)
            return response.blob_id
        finally:
            await channel.close()


async def get_blob_binary(blob_id: str) -> bytes:
    """Retrieve raw binary data by blob ID.

    Args:
        blob_id: The blob ID to retrieve.

    Returns:
        The raw bytes associated with the blob ID.

    Raises:
        RuntimeError: If blob service URL is not configured.
    """
    from stepflow_py.proto import GetBlobRequest
    from stepflow_py.proto.blobs_pb2_grpc import BlobServiceStub
    from stepflow_py.worker.observability import get_tracer

    url = _get_blob_url()
    tracer = get_tracer(__name__)
    with tracer.start_as_current_span(
        "get_blob_binary",
        attributes={"blob_id": blob_id},
    ):
        request = GetBlobRequest(blob_id=blob_id)
        channel = grpc.aio.insecure_channel(url)
        try:
            stub = BlobServiceStub(channel)
            response = await stub.GetBlob(request)
            if response.HasField("raw_data"):
                return response.raw_data
            elif response.HasField("json_data"):
                # Fallback: try to return as bytes if stored as JSON
                import json

                from stepflow_py.worker.task_handler import proto_value_to_python

                data = proto_value_to_python(response.json_data)
                return json.dumps(data).encode("utf-8")
            else:
                return b""
        finally:
            await channel.close()
