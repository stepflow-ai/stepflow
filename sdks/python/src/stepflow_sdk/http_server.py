# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

"""Streamable HTTP server implementation for the StepFlow Python SDK.

This module implements the Streamable HTTP transport according to the MCP spec,
replacing the previous HTTP + SSE session-based approach with simpler request/response.
"""

from __future__ import annotations

import asyncio
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
    Error,
    Message,
    MethodError,
    MethodRequest,
    MethodSuccess,
    Notification,
)
from .message_decoder import MessageDecoder
from .server import StepflowServer

try:
    import uvicorn
    from fastapi import FastAPI, Request
    from fastapi.responses import JSONResponse, StreamingResponse
except ImportError:
    print("Error: HTTP mode requires additional dependencies.", file=sys.stderr)
    print("Please install: pip install fastapi uvicorn", file=sys.stderr)
    sys.exit(1)


class StepflowStreamableHttpServer:
    """Streamable HTTP server for StepFlow components.

    Implements the Streamable HTTP transport specification:
    - Single POST endpoint accepting JSON-RPC Message objects
    - Returns 202 Accepted for responses/notifications
    - Returns 400 Bad Request for invalid input
    - For requests: Returns direct JSON or SSE stream based on Context usage
    - Uses RequestFuturesManager for bidirectional communication
    """

    def __init__(
        self,
        server: StepflowServer | None = None,
        host: str = "localhost",
        port: int = 8080,
    ):
        self.server = server or StepflowServer()
        self.host = host
        self.port = port
        self.app = FastAPI(title="StepFlow Streamable HTTP Server")
        self.message_decoder: MessageDecoder[asyncio.Future[Any]] = MessageDecoder()
        self._setup_routes()

    def _setup_routes(self):
        """Set up FastAPI routes for streamable HTTP transport."""

        @self.app.get("/health")
        async def health_check():
            """Health check endpoint for integration tests."""
            import datetime

            return JSONResponse(
                content={
                    "status": "healthy",
                    "timestamp": datetime.datetime.utcnow().isoformat() + "Z",
                    "service": "stepflow-python-http-server",
                },
                status_code=200,
                media_type="application/json",
            )

        @self.app.post("/")
        async def handle_message(request: Request):
            """Handle JSON-RPC messages according to Streamable HTTP specification."""
            try:
                # Parse request body using MessageDecoder for proper typing
                body_bytes = await request.body()

                # Set Accept header expectation
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

                # Try to parse JSON first to give proper parse errors
                try:
                    # Test JSON parsing to catch parse errors before MessageDecoder
                    msgspec.json.decode(body_bytes)
                except msgspec.DecodeError as e:
                    # Invalid JSON format - return parse error
                    return JSONResponse(
                        content={
                            "error": {
                                "code": -32700,
                                "message": f"Parse error: {str(e)}",
                            }
                        },
                        status_code=400,
                        media_type="application/json",
                    )

                # Use MessageDecoder for proper message parsing and result typing
                message, pending = self.message_decoder.decode(body_bytes)
                return await self._handle_message(message, pending)

            except msgspec.DecodeError as e:
                # Invalid JSON format - return plain error since no request ID
                return JSONResponse(
                    content={
                        "error": {"code": -32700, "message": f"Parse error: {str(e)}"}
                    },
                    status_code=400,
                    media_type="application/json",
                )
            except StepflowProtocolError as e:
                # MessageDecoder protocol error - return plain error since no request ID
                return JSONResponse(
                    content={"error": {"code": -32600, "message": str(e)}},
                    status_code=400,
                    media_type="application/json",
                )
            except Exception as e:
                # Internal server error - return plain error since no request ID
                return JSONResponse(
                    content={
                        "error": {
                            "code": -32603,
                            "message": f"Internal error: {str(e)}",
                        }
                    },
                    status_code=500,
                    media_type="application/json",
                )

    async def _handle_message(
        self, message: Message, pending: asyncio.Future[Any] | None
    ):
        """Handle a parsed JSON-RPC message according to type."""

        if isinstance(message, MethodRequest):
            # Handle method requests - may return JSON or SSE stream
            return await self._handle_request(message)

        elif isinstance(message, MethodError):
            assert pending is not None

            from .exceptions import StepflowProtocolError

            error_msg = f"JSON-RPC error {message.error.code}: {message.error.message}"
            if message.error.data is not None:
                error_msg += f" (data: {message.error.data})"
            pending.set_exception(StepflowProtocolError(error_msg))

            return JSONResponse(
                content="",
                status_code=202,  # Empty body for 202 Accepted
            )
        elif isinstance(message, MethodSuccess):
            assert pending is not None
            pending.set_result(message)
            return JSONResponse(
                content="",
                status_code=202,  # Empty body for 202 Accepted
            )

        elif isinstance(message, Notification):
            # Handle notifications - always return 202 Accepted
            # Note: We don't currently process notifications, just acknowledge them
            return JSONResponse(
                content="",
                status_code=202,  # Empty body for 202 Accepted
            )

        else:
            assert_never("Unhandled message type: {type(message)}")

    async def _handle_request(self, request: MethodRequest):
        """Handle a JSON-RPC method request."""
        try:
            # Check if this message requires bidirectional context
            needs_context = self.server.requires_context(request)

            if needs_context:
                # Create context for bidirectional communication, use streaming
                context = StepflowStreamingContext(self.message_decoder)
                return StreamingResponse(
                    self._execute_with_streaming_context(request, context),
                    media_type="text/event-stream",
                )
            else:
                # No context needed - delegate to core server
                result = await self.server.handle_message(request)

                # Return direct JSON response
                return JSONResponse(
                    content=msgspec.to_builtins(result), media_type="application/json"
                )

        except StepflowError as e:
            # Handle known StepFlow errors
            error_response = MethodError(
                jsonrpc="2.0",
                id=request.id,
                error=Error(code=e.code.value, message=e.message, data=e.data),
            )
            return JSONResponse(
                content=msgspec.to_builtins(error_response),
                status_code=400,
                media_type="application/json",
            )
        except Exception as e:
            # Handle unexpected errors
            error_response = MethodError(
                jsonrpc="2.0",
                id=request.id,
                error=Error(
                    code=-32603,  # Internal error
                    message=f"Internal error: {str(e)}",
                    data=None,
                ),
            )
            return JSONResponse(
                content=msgspec.to_builtins(error_response),
                status_code=500,
                media_type="application/json",
            )

    async def _execute_with_streaming_context(
        self, request: MethodRequest, context: StepflowContext
    ) -> AsyncGenerator[str]:
        """Execute request with streaming context via SSE.

        This method runs component execution and bidirectional message processing
        concurrently to avoid deadlock scenarios where components need to make
        bidirectional requests (like put_blob) before completing.
        """
        try:
            # Start component execution as a background task
            execution_task = asyncio.create_task(
                self.server.handle_message(request, context)
            )

            # Continuously process bidirectional messages while component runs
            while not execution_task.done():
                # Yield any pending bidirectional messages
                if isinstance(context, StepflowStreamingContext):
                    async for sse_event in context.get_pending_messages():
                        yield sse_event

                # Brief pause to avoid busy waiting
                await asyncio.sleep(0.01)

            # Component execution completed - get the result
            result = await execution_task

            # Yield any final pending bidirectional messages
            if isinstance(context, StepflowStreamingContext):
                async for sse_event in context.get_pending_messages():
                    yield sse_event

            # Yield final response
            sse_data = msgspec.json.encode(result).decode("utf-8")
            yield f"data: {sse_data}\n\n"

        except Exception as e:
            # If we have an execution task, make sure it's cancelled
            if "execution_task" in locals() and not execution_task.done():
                execution_task.cancel()
                try:
                    await execution_task
                except asyncio.CancelledError:
                    pass

            # Yield any pending messages first
            if isinstance(context, StepflowStreamingContext):
                async for sse_event in context.get_pending_messages():
                    yield sse_event

            # Yield error response
            error_response = MethodError(
                jsonrpc="2.0",
                id=request.id,
                error=Error(
                    code=-32603,
                    message=f"Request execution failed: {str(e)}",
                    data=None,
                ),
            )

            sse_data = msgspec.json.encode(error_response).decode("utf-8")
            yield f"data: {sse_data}\n\n"

    async def run(self):
        """Start the HTTP server."""
        print(
            f"Starting StepFlow Streamable HTTP server on {self.host}:{self.port}",
            file=sys.stderr,
        )

        # Initialize the base server
        self.server.set_initialized(True)

        # Start the HTTP server
        config = uvicorn.Config(
            app=self.app, host=self.host, port=self.port, log_level="info"
        )
        server = uvicorn.Server(config)
        await server.serve()


class StepflowStreamingContext(StepflowContext):
    """Context for components executed with streaming support.

    This context enables bidirectional communication by queuing
    JSON-RPC requests that will be sent via the SSE stream.
    Inherits from StepflowContext to reuse all request/response logic.
    """

    def __init__(self, message_decoder: MessageDecoder[asyncio.Future[Any]]):
        # Create standard asyncio queue for outgoing messages
        self._outgoing_queue = asyncio.Queue()

        # Initialize the base context with the shared message decoder
        super().__init__(
            outgoing_queue=self._outgoing_queue,
            message_decoder=message_decoder,
            session_id=None,
        )

    async def get_pending_messages(self) -> AsyncGenerator[str]:
        """Yield pending messages from the queue as SSE events."""
        # Drain all messages currently in the queue
        messages = []
        while not self._outgoing_queue.empty():
            try:
                message = self._outgoing_queue.get_nowait()
                messages.append(message)
            except asyncio.QueueEmpty:
                break

        # Stream all messages as SSE events
        for message in messages:
            sse_data = msgspec.json.encode(message).decode("utf-8")
            yield f"data: {sse_data}\n\n"


# For backward compatibility, alias the new class
StepflowHttpServer = StepflowStreamableHttpServer
