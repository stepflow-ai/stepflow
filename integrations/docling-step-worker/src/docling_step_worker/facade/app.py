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

"""FastAPI application presenting a docling-serve compatible API.

Translates incoming requests into Stepflow flow submissions and returns
responses in the exact shape callers expect from docling-serve.
"""

from __future__ import annotations

import json
import logging
import os
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

import httpx
import yaml
from fastapi import FastAPI, File, Form, HTTPException, UploadFile
from fastapi.responses import JSONResponse

from docling_step_worker.facade.translate import (
    flow_output_to_response,
    normalize_v1alpha_request,
    request_to_flow_input,
)

logger = logging.getLogger(__name__)

# Module-level singletons for FastAPI parameter defaults (ruff B008)
_FILE_PARAM = File(...)
_FORM_PARAM = Form(None)

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

STEPFLOW_SERVER_URL = os.getenv("STEPFLOW_SERVER_URL", "http://localhost:7840")
STEPFLOW_FLOW_NAME = os.getenv("STEPFLOW_FLOW_NAME", "docling-process-document")
FACADE_PORT = int(os.getenv("FACADE_PORT", "5001"))
REQUEST_TIMEOUT = int(os.getenv("STEPFLOW_REQUEST_TIMEOUT", "300"))

# Path to the flow YAML — checked at startup, embedded in Docker image
_FLOW_YAML_PATH = os.getenv(
    "STEPFLOW_FLOW_PATH",
    str(
        Path(__file__).resolve().parents[3] / "flows" / "docling-process-document.yaml"
    ),
)


def _load_flow_definition() -> dict[str, Any]:
    """Load the flow YAML from disk and parse it."""
    flow_path = Path(_FLOW_YAML_PATH)
    if not flow_path.exists():
        raise FileNotFoundError(f"Flow definition not found: {flow_path}")
    with open(flow_path) as f:
        return yaml.safe_load(f)


# ---------------------------------------------------------------------------
# Application state
# ---------------------------------------------------------------------------


class _State:
    """Mutable application state populated during lifespan."""

    client: httpx.AsyncClient
    flow_id: str


_state = _State()


# ---------------------------------------------------------------------------
# Lifespan — register the flow and obtain its ID
# ---------------------------------------------------------------------------


@asynccontextmanager
async def _lifespan(app: FastAPI):  # noqa: ARG001
    _state.client = httpx.AsyncClient(
        base_url=STEPFLOW_SERVER_URL,
        timeout=httpx.Timeout(REQUEST_TIMEOUT, connect=10.0),
    )

    # Register the flow with stepflow-server
    flow_def = _load_flow_definition()
    try:
        resp = await _state.client.post(
            "/api/v1/flows",
            json={"flow": flow_def},
        )
        resp.raise_for_status()
        data = resp.json()
        _state.flow_id = data["flowId"]
        logger.info(
            "Registered flow %s with ID %s",
            STEPFLOW_FLOW_NAME,
            _state.flow_id,
        )
    except Exception:
        logger.exception("Failed to register flow with stepflow-server")
        raise

    yield

    await _state.client.aclose()


# ---------------------------------------------------------------------------
# App factory
# ---------------------------------------------------------------------------


def create_app() -> FastAPI:
    """Create and return the FastAPI application."""
    app = FastAPI(
        title="Docling Document Conversion (Stepflow)",
        description=(
            "Drop-in replacement for docling-serve, backed by Stepflow orchestration."
        ),
        version="0.1.0",
        lifespan=_lifespan,
    )

    # -- v1 endpoints -------------------------------------------------------

    @app.post("/v1/convert/source")
    async def convert_source_v1(request: dict[str, Any]) -> JSONResponse:
        return await _handle_convert_source(request)

    @app.post("/v1/convert/file")
    async def convert_file_v1(
        files: UploadFile = _FILE_PARAM,
        options: str | None = _FORM_PARAM,
    ) -> JSONResponse:
        return await _handle_convert_file(files, options)

    # -- v1alpha backward-compat endpoints ----------------------------------

    @app.post("/v1alpha/convert/source")
    async def convert_source_v1alpha(request: dict[str, Any]) -> JSONResponse:
        normalized = normalize_v1alpha_request(request)
        return await _handle_convert_source(normalized)

    @app.post("/v1alpha/convert/file")
    async def convert_file_v1alpha(
        files: UploadFile = _FILE_PARAM,
        options: str | None = _FORM_PARAM,
    ) -> JSONResponse:
        return await _handle_convert_file(files, options)

    # -- Stubbed chunked endpoints (501) ------------------------------------

    @app.post("/v1/convert/chunked/{chunker_type}/source", status_code=501)
    async def chunked_source_v1(chunker_type: str) -> JSONResponse:
        return JSONResponse(
            status_code=501,
            content={
                "detail": f"Chunked conversion ({chunker_type}) not yet implemented"
            },
        )

    @app.post("/v1/convert/chunked/{chunker_type}/file", status_code=501)
    async def chunked_file_v1(chunker_type: str) -> JSONResponse:
        return JSONResponse(
            status_code=501,
            content={
                "detail": f"Chunked conversion ({chunker_type}) not yet implemented"
            },
        )

    # -- Health -------------------------------------------------------------

    @app.get("/health")
    async def health() -> JSONResponse:
        result: dict[str, Any] = {"facade": "ok"}
        try:
            resp = await _state.client.get("/api/v1/health", timeout=5.0)
            result["stepflow_server"] = "ok" if resp.status_code == 200 else "degraded"
        except Exception:
            result["stepflow_server"] = "unreachable"
        status = 200 if result.get("stepflow_server") == "ok" else 503
        return JSONResponse(content=result, status_code=status)

    return app


# ---------------------------------------------------------------------------
# Shared handler logic
# ---------------------------------------------------------------------------


async def _handle_convert_source(body: dict[str, Any]) -> JSONResponse:
    """Process a /convert/source request (v1 shape)."""
    sources = body.get("sources", [])
    options = body.get("options")

    try:
        flow_input = request_to_flow_input(sources=sources, options=options)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    return await _submit_and_respond(flow_input)


async def _handle_convert_file(
    file: UploadFile,
    options_json: str | None,
) -> JSONResponse:
    """Process a /convert/file request (multipart upload)."""
    file_bytes = await file.read()
    options = json.loads(options_json) if options_json else None

    flow_input = request_to_flow_input(
        file_bytes=file_bytes,
        filename=file.filename or "document.pdf",
        options=options,
    )
    return await _submit_and_respond(flow_input)


async def _submit_and_respond(flow_input: dict[str, Any]) -> JSONResponse:
    """Submit a flow run to stepflow-server and translate the result."""
    try:
        resp = await _state.client.post(
            "/api/v1/runs",
            json={
                "flowId": _state.flow_id,
                "input": [flow_input],
                "wait": True,
                "timeoutSecs": REQUEST_TIMEOUT,
            },
        )
    except httpx.TimeoutException as exc:
        raise HTTPException(
            status_code=504,
            detail="Stepflow run timed out",
        ) from exc
    except httpx.ConnectError as exc:
        raise HTTPException(
            status_code=502,
            detail="Cannot reach stepflow-server",
        ) from exc

    if resp.status_code >= 400:
        return JSONResponse(
            status_code=resp.status_code,
            content={"detail": "Stepflow server error", "upstream": resp.text},
        )

    run_data = resp.json()

    # Extract the flow result from the run response
    results = run_data.get("results") or []
    if not results:
        return JSONResponse(
            status_code=502,
            content={"detail": "No results returned from stepflow run"},
        )

    item = results[0]
    flow_result = item.get("result", {})
    outcome = flow_result.get("outcome")

    if outcome == "failed":
        error = flow_result.get("error", {})
        return JSONResponse(
            status_code=500,
            content={
                "detail": error.get("message", "Flow execution failed"),
                "error": error,
            },
        )

    flow_output = flow_result.get("result", {})
    response_body = flow_output_to_response(flow_output)
    return JSONResponse(content=response_body)


# ---------------------------------------------------------------------------
# Entry point (for local dev with full package installed)
# ---------------------------------------------------------------------------


def main() -> None:
    """CLI entry point for the facade server."""
    import uvicorn

    try:
        from stepflow_py.worker.observability import setup_observability

        setup_observability()
    except ImportError:
        logging.basicConfig(level=logging.INFO)
        logger.info("stepflow-py not available, skipping observability setup")

    uvicorn.run(
        "docling_step_worker.facade.app:create_app",
        factory=True,
        host="0.0.0.0",
        port=FACADE_PORT,
        log_level="info",
    )


if __name__ == "__main__":
    main()
