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

"""Pure translation functions between docling-serve API shapes and Stepflow flow I/O.

No I/O, no imports from ``docling`` or ``docling_core``.
"""

from __future__ import annotations

import base64
from typing import Any

# ---------------------------------------------------------------------------
# v1alpha -> v1 request normalization
# ---------------------------------------------------------------------------


def normalize_v1alpha_request(body: dict[str, Any]) -> dict[str, Any]:
    """Convert a v1alpha request body to v1 shape.

    v1alpha used separate ``http_sources`` and ``file_sources`` lists.
    v1 unifies them under ``sources`` with a ``kind`` discriminator.

    If the body already contains ``sources``, it is returned unchanged.
    """
    if "sources" in body:
        return body

    sources: list[dict[str, Any]] = []

    for hs in body.pop("http_sources", None) or []:
        sources.append({"kind": "http", **hs})

    for fs in body.pop("file_sources", None) or []:
        sources.append({"kind": "file", **fs})

    body["sources"] = sources
    return body


# ---------------------------------------------------------------------------
# Request -> flow input
# ---------------------------------------------------------------------------


def request_to_flow_input(
    *,
    sources: list[dict[str, Any]] | None = None,
    options: dict[str, Any] | None = None,
    file_bytes: bytes | None = None,
    filename: str = "document.pdf",
) -> dict[str, Any]:
    """Build the flow input dict for ``docling-process-document``.

    Exactly one of *sources* or *file_bytes* must be provided.

    Parameters
    ----------
    sources:
        v1 ``sources`` array (each entry has ``kind``, ``url``/``base64_string``, etc.).
    options:
        docling-serve compatible conversion options.  ``to_formats`` and
        ``image_export_mode`` may appear at the top level of *options* or as
        dedicated fields; we hoist them to the flow's top-level input.
    file_bytes:
        Raw file content from a multipart upload.
    filename:
        Original filename (used for multipart uploads).
    """
    flow_input: dict[str, Any] = {}

    if file_bytes is not None:
        flow_input["source"] = base64.b64encode(file_bytes).decode("ascii")
        flow_input["source_kind"] = "base64"
    elif sources:
        src = sources[0]
        kind = src.get("kind", "http")
        if kind == "http":
            flow_input["source"] = src["url"]
            flow_input["source_kind"] = "url"
        elif kind == "file":
            flow_input["source"] = src["base64_string"]
            flow_input["source_kind"] = "base64"
        else:
            flow_input["source"] = src.get("url") or src.get("base64_string", "")
            flow_input["source_kind"] = "url" if "url" in src else "base64"
    else:
        raise ValueError("Either sources or file_bytes must be provided")

    # Hoist to_formats / image_export_mode from options or top-level.
    # All optional fields must be present in flow input (even as None)
    # because the flow's $input references fail on missing keys.
    opts = dict(options) if options else {}
    to_formats = opts.pop("to_formats", None)
    image_export_mode = opts.pop("image_export_mode", None)

    # Apply stock docling-serve defaults for missing options.
    # images_scale defaults to 2.0 in stock docling-serve; without this the
    # underlying docling library uses a lower default (~0.83) producing
    # noticeably smaller embedded images.
    opts.setdefault("images_scale", 2.0)

    flow_input["to_formats"] = to_formats or ["md"]
    flow_input["image_export_mode"] = image_export_mode or "embedded"
    flow_input["options"] = opts if opts else None
    flow_input["chunk_options"] = None

    return flow_input


# ---------------------------------------------------------------------------
# Flow output -> docling-serve response
# ---------------------------------------------------------------------------


def flow_output_to_response(flow_output: dict[str, Any]) -> dict[str, Any]:
    """Convert Stepflow flow output to a ``ConvertDocumentResponse``-shaped dict.

    The flow output contains:
    - ``document``: ExportDocumentResponse (md_content, html_content, etc.)
    - ``status``: ``"success"`` or ``"failure"``
    - ``errors``: list of error dicts
    - ``processing_time``: float seconds
    - ``timings``: dict of phase timings
    - ``chunks``: list (unused here)
    - ``classification``: dict (unused here)
    """
    return {
        "document": flow_output.get("document", {}),
        "status": flow_output.get("status", "success"),
        "errors": flow_output.get("errors", []),
        "processing_time": flow_output.get("processing_time", 0.0),
        "timings": flow_output.get("timings", {}),
    }


# ---------------------------------------------------------------------------
# Stepflow run status -> async poll response
# ---------------------------------------------------------------------------

# Map stepflow run statuses to docling-serve TaskStatus values.
_STATUS_MAP: dict[str, str] = {
    "running": "started",
    "pending": "pending",
    "completed": "success",
    "failed": "failure",
}


def run_status_to_poll_response(
    run_data: dict[str, Any],
    task_type: str = "convert",
) -> dict[str, Any]:
    """Convert a Stepflow run status to a docling-serve ``TaskStatusResponse``.

    Matches the docling-serve response shape:
    - ``task_id``: str
    - ``task_type``: "convert" | "chunk"
    - ``task_status``: "pending" | "started" | "success" | "failure"
    - ``task_position``: optional int (queue position)
    - ``task_meta``: optional processing progress
    - ``error_message``: optional str on failure
    """
    stepflow_status = run_data.get("status", "pending")
    task_status = _STATUS_MAP.get(stepflow_status, "pending")

    response: dict[str, Any] = {
        "task_id": run_data.get("runId", ""),
        "task_type": task_type,
        "task_status": task_status,
        "task_position": None,
        "task_meta": None,
        "error_message": None,
    }

    if task_status == "failure":
        results = run_data.get("results") or []
        if results:
            item = results[0]
            flow_result = item.get("result", {})
            error = flow_result.get("error", {})
            response["error_message"] = error.get(
                "message", "Flow execution failed"
            )

    return response


def run_result_to_convert_response(run_data: dict[str, Any]) -> dict[str, Any] | None:
    """Extract the ``ConvertDocumentResponse`` from a completed run.

    Returns ``None`` if the run has no results yet.
    """
    results = run_data.get("results") or []
    if not results:
        return None
    item = results[0]
    flow_result = item.get("result", {})
    flow_output = flow_result.get("result", {})
    return flow_output_to_response(flow_output)
