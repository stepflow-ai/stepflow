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

"""Blob store and document source helpers.

Utilities for reading/writing documents through the blob store
and constructing docling source objects for in-memory processing.
"""

from __future__ import annotations

import base64
import logging
from io import BytesIO
from typing import Any

import httpx
from docling.datamodel.base_models import DocumentStream
from stepflow_py.worker import blob_store
from stepflow_py.worker.blob_ref import is_blob_ref

from docling_step_worker.exceptions import BlobStoreError

logger = logging.getLogger(__name__)


async def get_document_bytes(
    source: str | dict[str, Any], source_kind: str, filename: str | None = None
) -> bytes:
    """Retrieve document bytes from blob store, URL, or base64 string.

    Handles Stepflow's automatic blob externalization: when the orchestrator
    replaces a large input value with a ``{"$blob": "<sha256>", "size": N}``
    reference, this function transparently resolves it before processing.

    Args:
        source: blob ref string ("blob:sha256:..."), URL string, base64 string,
            or a Stepflow ``$blob`` reference dict.
        source_kind: "blob", "url", or "base64"
        filename: Optional filename hint (unused for blob, used for logging)

    Returns:
        Raw document bytes

    Raises:
        BlobStoreError: If blob retrieval fails
        httpx.HTTPError: If URL download fails
    """
    # Resolve Stepflow $blob references.  The orchestrator may blobify large
    # input fields (e.g. base64-encoded PDFs) and replace them with
    # {"$blob": "<sha256>", "size": N}.  We fetch the original value from the
    # blob store before proceeding with the normal source_kind logic.
    if is_blob_ref(source):
        blob_id = source["$blob"]  # type: ignore[index]
        logger.debug(
            "Resolving $blob reference %s (source_kind=%s)", blob_id[:12], source_kind
        )
        try:
            source = await blob_store.get_blob(blob_id)
        except Exception as e:
            raise BlobStoreError(
                f"Failed to resolve $blob reference {blob_id}: {e}"
            ) from e

    if source_kind == "blob":
        try:
            data = await blob_store.get_blob_binary(source)
            return data
        except Exception as e:
            raise BlobStoreError(f"Failed to retrieve blob {source}: {e}") from e
    elif source_kind == "url":
        async with httpx.AsyncClient(timeout=120.0) as client:
            resp = await client.get(source)
            resp.raise_for_status()
            return resp.content
    elif source_kind == "base64":
        return base64.b64decode(source)
    else:
        raise BlobStoreError(f"Unknown source_kind: {source_kind}")


def bytes_to_document_stream(
    data: bytes, filename: str = "document.pdf"
) -> DocumentStream:
    """Wrap raw bytes in a docling DocumentStream for in-memory processing.

    This is the preferred way to pass documents to DocumentConverter.convert()
    when we already have bytes (from blob store). No temp files needed.

    Args:
        data: Raw document bytes
        filename: Filename to associate with the stream

    Returns:
        DocumentStream wrapping the bytes
    """
    return DocumentStream(name=filename, stream=BytesIO(data))


async def put_document_blob(doc_dict: dict[str, Any]) -> str:
    """Store a DoclingDocument dict in blob store, return blob ref.

    Args:
        doc_dict: DoclingDocument serialized via export_to_dict()

    Returns:
        Blob reference string (e.g., "sha256:abc123...")

    Raises:
        BlobStoreError: If blob storage fails
    """
    try:
        return await blob_store.put_blob(doc_dict)
    except Exception as e:
        raise BlobStoreError(f"Failed to store document blob: {e}") from e


async def get_document_dict(blob_ref: str) -> dict[str, Any]:
    """Retrieve a DoclingDocument dict from blob store.

    Args:
        blob_ref: Blob reference string

    Returns:
        DoclingDocument dict

    Raises:
        BlobStoreError: If blob retrieval fails
    """
    try:
        data = await blob_store.get_blob(blob_ref)
        if not isinstance(data, dict):
            raise BlobStoreError(
                f"Expected dict from blob store, got {type(data).__name__}"
            )
        return data
    except BlobStoreError:
        raise
    except Exception as e:
        raise BlobStoreError(f"Failed to retrieve document blob {blob_ref}: {e}") from e
