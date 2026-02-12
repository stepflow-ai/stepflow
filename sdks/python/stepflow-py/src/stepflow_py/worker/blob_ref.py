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

"""Blob reference (`$blob` sentinel) for representing blobified data in JSON values.

A blob ref is a JSON object with a ``$blob`` key that indicates "this value is stored
in the blob store". This is a runtime data convention, distinct from workflow-definition
ValueExpr (``$step``, ``$input``, ``$variable``).

Example::

    {"$blob": "<sha256hex>", "blobType": "data", "size": 12345}
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any

from stepflow_py.api.models.blob_type import BlobType

BLOB_REF_KEY = "$blob"


@dataclass
class BlobRef:
    """A reference to a blob stored in the blob store."""

    blob_id: str
    blob_type: BlobType = BlobType.DATA
    size: int | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to a JSON-serializable dict."""
        d: dict[str, Any] = {BLOB_REF_KEY: self.blob_id}
        if self.blob_type != BlobType.DATA:
            d["blobType"] = self.blob_type.value
        if self.size is not None:
            d["size"] = self.size
        return d

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> BlobRef | None:
        """Try to parse a dict as a blob ref. Returns None if not a blob ref."""
        if not isinstance(d, dict) or BLOB_REF_KEY not in d:
            return None
        blob_id = d[BLOB_REF_KEY]
        if not isinstance(blob_id, str) or len(blob_id) != 64:
            return None
        blob_type_str = d.get("blobType", "data")
        try:
            blob_type = BlobType(blob_type_str)
        except ValueError:
            blob_type = BlobType.DATA
        size = d.get("size")
        return cls(blob_id=blob_id, blob_type=blob_type, size=size)


def is_blob_ref(value: Any) -> bool:
    """Quick check whether a value is a blob ref (dict with ``$blob`` key)."""
    return isinstance(value, dict) and BLOB_REF_KEY in value


async def blobify_inputs(
    input_data: Any,
    threshold: int,
) -> tuple[Any, list[str]]:
    """Replace large top-level fields in an input dict with blob refs.

    Checks each top-level field individually against the threshold.
    Fields whose JSON serialization exceeds the threshold are stored as blobs
    and replaced with blob refs.

    Args:
        input_data: The input dict (or any value — non-dicts pass through).
        threshold: Byte size threshold. 0 means disabled.

    Returns:
        A tuple of (possibly modified input, list of created blob IDs).
    """
    from stepflow_py.worker import blob_store

    if threshold == 0 or not isinstance(input_data, dict):
        return input_data, []

    created_ids: list[str] = []
    result: dict[str, Any] = {}

    for key, value in input_data.items():
        if is_blob_ref(value):
            result[key] = value
            continue
        serialized = json.dumps(value, separators=(",", ":")).encode("utf-8")
        size = len(serialized)
        if size > threshold:
            blob_id = await blob_store.put_blob(value)
            ref = BlobRef(blob_id=blob_id, blob_type=BlobType.DATA, size=size)
            result[key] = ref.to_dict()
            created_ids.append(blob_id)
        else:
            result[key] = value

    return result, created_ids


async def resolve_blob_refs(
    value: Any,
    _depth: int = 0,
) -> Any:
    """Recursively replace blob refs in a JSON value with their inline data.

    Args:
        value: The JSON value to resolve.

    Returns:
        The value with all blob refs replaced by their inline data.
    """
    from stepflow_py.worker import blob_store

    max_depth = 8
    if _depth > max_depth:
        return value

    if isinstance(value, dict):
        blob_ref = BlobRef.from_dict(value)
        if blob_ref is not None:
            resolved = await blob_store.get_blob(blob_ref.blob_id)
            return await resolve_blob_refs(resolved, _depth + 1)
        # Not a blob ref — recurse into fields
        return {k: await resolve_blob_refs(v, _depth + 1) for k, v in value.items()}

    if isinstance(value, list):
        return [await resolve_blob_refs(item, _depth + 1) for item in value]

    return value
