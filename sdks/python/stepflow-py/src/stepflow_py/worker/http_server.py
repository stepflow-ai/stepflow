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

"""Streamable HTTP server implementation for the Stepflow Python SDK.

This module implements the Streamable HTTP transport according to the MCP spec,
replacing the previous HTTP + SSE session-based approach with simpler request/response.
"""

from __future__ import annotations

import asyncio
import logging
import sys
from collections.abc import AsyncGenerator
from typing import Any, assert_never

import msgspec

from .context import StepflowContext
from .exceptions import (
    StepflowError,
    StepflowProtocolError,
)
from .generated_protocol import (
    ComponentExecuteParams,
    Error,
    Message,
    Method,
    MethodError,
    MethodRequest,
    MethodSuccess,
    Notification,
    RequestId,
)
from .message_decoder import MessageDecoder
from .server import StepflowServer

logger = logging.getLogger(__name__)

try:
    import uvicorn
    from fastapi import FastAPI, Request
    from fastapi.responses import JSONResponse, StreamingResponse
except ImportError:
    print("Error: HTTP mode requires additional dependencies.")
    print("Please install: pip install fastapi uvicorn")
    sys.exit(1)


def _generate_instance_id() -> str:
    """Generate a default instance ID using UUID."""
    import uuid

    return uuid.uuid4().hex[:16]


class _HttpServerContext:
    """Internal context for HTTP server route handlers."""

    def __init__(self, server: StepflowServer, instance_id: str):
        self.server = server
        self.instance_id = instance_id
        self.message_decoder: MessageDecoder[asyncio.Future[Any]] = MessageDecoder()

    def create_error_response(
        self,
        request_id: RequestId | None,
        status_code: int,
        error_code: int,
        error_message: str,
        data: Any = None,
    ) -> JSONResponse:
        """Create an error response."""
        if request_id is not None:
            # Have request ID - create proper JSON-RPC MethodError
            error_response = MethodError(
                jsonrpc="2.0",
                id=request_id,
                error=Error(code=error_code, message=error_message, data=data),
            )
            content = msgspec.to_builtins(error_response)
        else:
            # No request ID - create plain error object
            content = {"error": {"code": error_code, "message": error_message}}
            if data is not None:
                content["error"]["data"] = data

        return JSONResponse(
            content=content,
            status_code=status_code,
            media_type="application/json",
        )

    def create_sse_error_event(
        self,
        request_id: RequestId,
        error_code: int,
        error_message: str,
        data: Any = None,
    ) -> str:
        """Create an SSE error event for streaming responses."""
        error_response = MethodError(
            jsonrpc="2.0",
            id=request_id,
            error=Error(code=error_code, message=error_message, data=data),
        )
        sse_data = msgspec.json.encode(error_response).decode("utf-8")
        return f"data: {sse_data}\n\n"

    async def handle_message_internal(
        self, message: Message, pending: asyncio.Future[Any] | None
    ):
        """Handle a parsed JSON-RPC message according to type."""
        if isinstance(message, MethodRequest):
            return await self.handle_request(message)
        elif isinstance(message, MethodError):
            assert pending is not None
            error_msg = f"JSON-RPC error {message.error.code}: {message.error.message}"
            if message.error.data is not None:
                error_msg += f" (data: {message.error.data})"
            pending.set_exception(StepflowProtocolError(error_msg))
            return JSONResponse(content="", status_code=202)
        elif isinstance(message, MethodSuccess):
            assert pending is not None
            pending.set_result(message)
            return JSONResponse(content="", status_code=202)
        elif isinstance(message, Notification):
            return JSONResponse(content="", status_code=202)
        else:
            assert_never("Unhandled message type: {type(message)}")

    async def handle_request(self, request: MethodRequest):
        """Handle a JSON-RPC method request."""
        try:
            needs_context = self.server.requires_context(request)

            if needs_context:
                outgoing_queue: asyncio.Queue[MethodRequest | None] = asyncio.Queue()

                step_id = None
                run_id = None
                flow_id = None
                attempt = 1
                observability = None
                if request.method == Method.components_execute:
                    assert isinstance(request.params, ComponentExecuteParams)
                    attempt = request.params.attempt
                    observability = request.params.observability
                    step_id = observability.step_id
                    run_id = observability.run_id
                    flow_id = observability.flow_id

                context = StepflowContext(
                    outgoing_queue=outgoing_queue,
                    message_decoder=self.message_decoder,
                    session_id=None,
                    step_id=step_id,
                    run_id=run_id,
                    flow_id=flow_id,
                    attempt=attempt,
                    observability=observability,
                )
                return StreamingResponse(
                    self.execute_with_streaming_context(
                        request, context, outgoing_queue
                    ),
                    media_type="text/event-stream",
                    headers={"Stepflow-Instance-Id": self.instance_id},
                )
            else:
                result = await self.server.handle_message(request)
                assert result is not None
                return JSONResponse(
                    content=msgspec.to_builtins(result), media_type="application/json"
                )
        except StepflowError as e:
            return self.create_error_response(
                request_id=request.id,
                status_code=400,
                error_code=e.code.value,
                error_message=e.message,
                data=e.data,
            )
        except Exception as e:
            return self.create_error_response(
                request_id=request.id,
                status_code=500,
                error_code=-32603,
                error_message=f"Internal error: {str(e)}",
            )

    async def execute_with_streaming_context(
        self,
        request: MethodRequest,
        context: StepflowContext,
        outgoing_queue: asyncio.Queue[MethodRequest | None],
    ) -> AsyncGenerator[str]:
        """Execute request with streaming context via SSE."""

        async def execute_and_shutdown_queue():
            try:
                result = await self.server.handle_message(request, context)
                assert result is not None
                return result
            finally:
                await outgoing_queue.put(None)

        execution_task = asyncio.create_task(execute_and_shutdown_queue())

        try:
            while True:
                message = await outgoing_queue.get()
                if message is None:
                    break
                sse_data = msgspec.json.encode(message).decode("utf-8")
                yield f"data: {sse_data}\n\n"

            assert execution_task.done()
            result = await execution_task
            sse_data = msgspec.json.encode(result).decode("utf-8")
            yield f"data: {sse_data}\n\n"
        except Exception as e:
            if not execution_task.done():
                execution_task.cancel()
            yield self.create_sse_error_event(
                request_id=request.id,
                error_code=-32603,
                error_message=f"Request execution failed: {str(e)}",
            )


def _create_app(ctx: _HttpServerContext) -> FastAPI:
    """Create FastAPI app with routes for the given server context."""
    app = FastAPI(title="Stepflow Streamable HTTP Server")

    @app.get("/health")
    async def health_check():
        """Health check endpoint for integration tests and load balancers."""
        import datetime

        return JSONResponse(
            content={
                "status": "healthy",
                "instanceId": ctx.instance_id,
                "timestamp": datetime.datetime.utcnow().isoformat() + "Z",
                "service": "stepflow-python-http-server",
            },
            status_code=200,
            media_type="application/json",
        )

    @app.post("/")
    async def handle_message(request: Request):
        """Handle JSON-RPC messages according to Streamable HTTP specification."""
        try:
            content_type = request.headers.get("content-type", "")
            if "application/json" not in content_type:
                return JSONResponse(
                    content={"error": "Content-Type must be application/json"},
                    status_code=415,
                    media_type="application/json",
                )

            accept_header = request.headers.get("accept", "")
            if not (
                "application/json" in accept_header
                and "text/event-stream" in accept_header
            ):
                return JSONResponse(
                    content={
                        "error": (
                            "Accept header must include "
                            "application/json and text/event-stream"
                        )
                    },
                    status_code=406,
                    media_type="application/json",
                )

            body_bytes = await request.body()
            message, pending = ctx.message_decoder.decode(body_bytes)
            return await ctx.handle_message_internal(message, pending)
        except StepflowProtocolError as e:
            return ctx.create_error_response(
                request_id=None,
                status_code=400,
                error_code=-32600,
                error_message=str(e),
            )
        except Exception as e:
            return ctx.create_error_response(
                request_id=None,
                status_code=500,
                error_code=-32603,
                error_message=f"Internal error: {str(e)}",
            )

    return app


class _StepflowUvicornServer(uvicorn.Server):
    """Custom uvicorn server that announces the port after binding."""

    async def startup(self, sockets: list | None = None) -> None:
        """Override startup to announce port after binding."""
        await super().startup(sockets)

        # Announce the actual port to stdout for the orchestrator
        for server in self.servers:
            for sock in server.sockets:
                addr = sock.getsockname()
                actual_port = addr[1]
                import json

                announcement = {"port": actual_port}
                print(json.dumps(announcement), flush=True)
                logger.info(f"Server listening on port {actual_port}")
                return


async def run_http_server(
    server: StepflowServer,
    host: str = "127.0.0.1",
    port: int = 0,
    workers: int = 3,
    backlog: int = 128,
    timeout_keep_alive: int = 5,
    instance_id: str | None = None,
) -> None:
    """Start the HTTP server.

    When port is 0, the server binds to an available port and announces
    the actual port via stdout in JSON format: {"port": N}
    """
    if instance_id is None:
        instance_id = _generate_instance_id()
    ctx = _HttpServerContext(server, instance_id)
    app = _create_app(ctx)

    logger.info(f"Starting Stepflow Streamable HTTP server on {host}:{port}")
    logger.info(f"  Workers: {workers}")
    logger.info(f"  Backlog: {backlog}")
    logger.info(f"  Keep-alive timeout: {timeout_keep_alive}s")

    server.set_initialized(True)

    config = uvicorn.Config(
        app=app,
        host=host,
        port=port,
        log_level="warning",
        workers=workers,
        backlog=backlog,
        timeout_keep_alive=timeout_keep_alive,
    )
    uvicorn_server = _StepflowUvicornServer(config)
    await uvicorn_server.serve()


def create_test_app(server: StepflowServer, instance_id: str | None = None) -> FastAPI:
    """Create a FastAPI app for testing purposes.

    Args:
        server: StepflowServer instance with registered components
        instance_id: Optional instance ID (auto-generated if not provided)

    Returns:
        FastAPI application instance for testing
    """
    if instance_id is None:
        instance_id = _generate_instance_id()
    ctx = _HttpServerContext(server, instance_id)
    return _create_app(ctx)
