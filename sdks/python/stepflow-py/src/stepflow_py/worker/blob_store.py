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

"""Module-level blob store API backed by contextvars.

This module provides the canonical blob API functions that can be used by any
code without requiring a StepflowContext parameter. The blob API URL and HTTP
client are configured via contextvars during server initialization.

Usage::

    from stepflow_py.worker import blob_store

    # In any async code (component, executor, framework):
    blob_id = await blob_store.put_blob({"key": "value"})
    data = await blob_store.get_blob(blob_id)
"""

from __future__ import annotations

from contextvars import ContextVar, Token
from dataclasses import dataclass
from typing import Any

from stepflow_py.api.models.blob_type import BlobType

# ---------------------------------------------------------------------------
# Contextvars – private; callers use configure() / reset()
# ---------------------------------------------------------------------------

_blob_api_url: ContextVar[str | None] = ContextVar(
    "stepflow_blob_api_url", default=None
)
_blob_http_client: ContextVar[Any] = ContextVar(
    "stepflow_blob_http_client", default=None
)


# ---------------------------------------------------------------------------
# Configuration API
# ---------------------------------------------------------------------------


@dataclass
class BlobStoreTokens:
    """Opaque tokens returned by :func:`configure` for later :func:`reset`."""

    url_token: Token[str | None] | None = None
    client_token: Token[Any] | None = None


def configure(
    *,
    blob_api_url: str | None = None,
    http_client: Any = None,
) -> BlobStoreTokens:
    """Configure the blob store for the current async scope.

    Only non-``None`` arguments are applied; the rest are left unchanged.
    Call :func:`reset` with the returned tokens to restore the previous values.
    """
    url_token = _blob_api_url.set(blob_api_url) if blob_api_url is not None else None
    client_token = (
        _blob_http_client.set(http_client) if http_client is not None else None
    )
    return BlobStoreTokens(url_token=url_token, client_token=client_token)


def reset(tokens: BlobStoreTokens) -> None:
    """Restore previous blob store configuration."""
    if tokens.url_token is not None:
        _blob_api_url.reset(tokens.url_token)
    if tokens.client_token is not None:
        _blob_http_client.reset(tokens.client_token)


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def is_configured() -> bool:
    """Return True if the blob store has been configured with a URL and client."""
    return _blob_api_url.get() is not None and _blob_http_client.get() is not None


def _get_blob_api_url(blob_id: str | None = None) -> str:
    """Get the blob API URL, optionally with a blob ID path appended."""
    url = _blob_api_url.get()
    if url is None:
        raise RuntimeError(
            "Blob API URL not configured. "
            "Ensure the orchestrator is configured with blobApi.url"
        )
    if blob_id is not None:
        return f"{url}/{blob_id}"
    return url


def _get_client() -> Any:
    client = _blob_http_client.get()
    if client is None:
        raise RuntimeError(
            "HTTP client not configured. "
            "Blob store must be used within a running Stepflow server."
        )
    return client


async def put_blob(data: Any, blob_type: BlobType = BlobType.DATA) -> str:
    """Store JSON data as a blob and return its content-based ID (SHA-256).

    Args:
        data: The JSON-serializable data to store.
        blob_type: The type of blob to store (flow or data).

    Returns:
        The blob ID (SHA-256 hash) for the stored data.

    Raises:
        RuntimeError: If blob store is not configured.
    """
    from stepflow_py.worker.observability import get_tracer

    url = _get_blob_api_url()
    client = _get_client()

    tracer = get_tracer(__name__)
    with tracer.start_as_current_span(
        "put_blob",
        attributes={"blob_type": blob_type.value},
    ):
        resp = await client.post(url, json={"data": data, "blobType": blob_type.value})
        resp.raise_for_status()
        blob_id: str = resp.json()["blobId"]
        return blob_id


async def get_blob(blob_id: str) -> Any:
    """Retrieve JSON data by blob ID.

    Args:
        blob_id: The blob ID to retrieve.

    Returns:
        The JSON data associated with the blob ID.

    Raises:
        RuntimeError: If blob store is not configured.
    """
    from stepflow_py.worker.observability import get_tracer

    url = _get_blob_api_url(blob_id)
    client = _get_client()

    tracer = get_tracer(__name__)
    with tracer.start_as_current_span(
        "get_blob",
        attributes={"blob_id": blob_id},
    ):
        resp = await client.get(url)
        resp.raise_for_status()
        return resp.json()["data"]


async def get_blobs(blob_ids: set[str]) -> dict[str, Any]:
    """Retrieve multiple blobs in parallel.

    Fetches all blob IDs concurrently using ``asyncio.gather``.

    Args:
        blob_ids: Set of blob IDs to retrieve.

    Returns:
        A mapping from blob ID to its JSON data.

    Raises:
        RuntimeError: If blob store is not configured.
    """
    import asyncio

    if not blob_ids:
        return {}

    ids = list(blob_ids)
    results = await asyncio.gather(*(get_blob(bid) for bid in ids))
    return dict(zip(ids, results, strict=False))


async def put_blob_binary(data: bytes) -> str:
    """Store raw binary data as a blob and return its content-based ID.

    The data is base64-encoded for transport. The blob ID is the SHA-256 hash
    of the raw bytes (not the base64 encoding).

    Args:
        data: The raw bytes to store.

    Returns:
        The blob ID (SHA-256 hash) for the stored data.

    Raises:
        RuntimeError: If blob store is not configured.
    """
    import pybase64

    from stepflow_py.worker.observability import get_tracer

    url = _get_blob_api_url()
    client = _get_client()
    b64_str = pybase64.standard_b64encode(data).decode("ascii")

    tracer = get_tracer(__name__)
    with tracer.start_as_current_span(
        "put_blob_binary",
        attributes={"size": len(data)},
    ):
        resp = await client.post(
            url, json={"data": b64_str, "blobType": BlobType.BINARY.value}
        )
        resp.raise_for_status()
        blob_id: str = resp.json()["blobId"]
        return blob_id


async def get_blob_binary(blob_id: str) -> bytes:
    """Retrieve raw binary data by blob ID.

    The stored base64 data is decoded back to raw bytes.

    Args:
        blob_id: The blob ID to retrieve.

    Returns:
        The raw bytes associated with the blob ID.

    Raises:
        RuntimeError: If blob store is not configured.
    """
    import pybase64

    from stepflow_py.worker.observability import get_tracer

    url = _get_blob_api_url(blob_id)
    client = _get_client()

    tracer = get_tracer(__name__)
    with tracer.start_as_current_span(
        "get_blob_binary",
        attributes={"blob_id": blob_id},
    ):
        resp = await client.get(url)
        resp.raise_for_status()
        b64_str = resp.json()["data"]
        return pybase64.standard_b64decode(b64_str)
