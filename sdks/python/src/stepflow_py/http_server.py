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

try:
    import uvicorn
    from fastapi import FastAPI, Request
    from fastapi.responses import JSONResponse, StreamingResponse
except ImportError:
    print("Error: HTTP mode requires additional dependencies.", file=sys.stderr)
    print("Please install: pip install fastapi uvicorn", file=sys.stderr)
    sys.exit(1)


class StepflowHttpServer:
    """Streamable HTTP server for Stepflow components.

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
        instance_id: str | None = None,
    ):
        self.server = server or StepflowServer()
        self.host = host
        self.port = port
        self.instance_id = instance_id or self._generate_instance_id()
        self.app = FastAPI(title="Stepflow Streamable HTTP Server")
        self.message_decoder: MessageDecoder[asyncio.Future[Any]] = MessageDecoder()
        self._setup_routes()

    def _generate_instance_id(self) -> str:
        """Generate a default instance ID using UUID."""
        import uuid

        return uuid.uuid4().hex[:16]

    def _create_error_response(
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

    def _create_sse_error_event(
        self,
        request_id: RequestId,
        error_code: int,
        error_message: str,
        data: Any = None,
    ) -> str:
        """Create an SSE error event for streaming responses."""
        # Reuse the error creation logic, but extract just the MethodError content
        error_response = MethodError(
            jsonrpc="2.0",
            id=request_id,
            error=Error(code=error_code, message=error_message, data=data),
        )
        sse_data = msgspec.json.encode(error_response).decode("utf-8")
        return f"data: {sse_data}\n\n"

    def _setup_routes(self):
        """Set up FastAPI routes for streamable HTTP transport."""

        @self.app.get("/health")
        async def health_check():
            """Health check endpoint for integration tests and load balancers."""
            import datetime

            return JSONResponse(
                content={
                    "status": "healthy",
                    "instanceId": self.instance_id,
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
                # Verify Content-Type header.
                content_type = request.headers.get("content-type", "")
                if "application/json" not in content_type:
                    return JSONResponse(
                        content={"error": "Content-Type must be application/json"},
                        status_code=415,
                        media_type="application/json",
                    )

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

                # Parse request body using MessageDecoder for proper typing
                body_bytes = await request.body()

                # Use MessageDecoder for proper message parsing and result typing
                message, pending = self.message_decoder.decode(body_bytes)
                return await self._handle_message(message, pending)

            except StepflowProtocolError as e:
                # MessageDecoder protocol error - no request ID available
                return self._create_error_response(
                    request_id=None,
                    status_code=400,
                    error_code=-32600,
                    error_message=str(e),
                )
            except Exception as e:
                # Internal server error - no request ID available
                return self._create_error_response(
                    request_id=None,
                    status_code=500,
                    error_code=-32603,
                    error_message=f"Internal error: {str(e)}",
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
                outgoing_queue: asyncio.Queue[MethodRequest | None] = asyncio.Queue()

                # Extract execution parameters for component execution requests
                step_id = None
                run_id = None
                flow_id = None
                attempt = 1
                if request.method == Method.components_execute:
                    assert isinstance(request.params, ComponentExecuteParams)
                    step_id = request.params.step_id
                    run_id = request.params.run_id
                    flow_id = request.params.flow_id
                    attempt = request.params.attempt

                context = StepflowContext(
                    outgoing_queue=outgoing_queue,
                    message_decoder=self.message_decoder,
                    session_id=None,
                    step_id=step_id,
                    run_id=run_id,
                    flow_id=flow_id,
                    attempt=attempt,
                )
                return StreamingResponse(
                    self._execute_with_streaming_context(
                        request, context, outgoing_queue
                    ),
                    media_type="text/event-stream",
                    headers={"Stepflow-Instance-Id": self.instance_id},
                )
            else:
                # No context needed - delegate to core server
                result = await self.server.handle_message(request)
                assert result is not None

                # Return direct JSON response
                return JSONResponse(
                    content=msgspec.to_builtins(result), media_type="application/json"
                )

        except StepflowError as e:
            # Handle known Stepflow errors
            return self._create_error_response(
                request_id=request.id,
                status_code=400,
                error_code=e.code.value,
                error_message=e.message,
                data=e.data,
            )
        except Exception as e:
            # Handle unexpected errors
            return self._create_error_response(
                request_id=request.id,
                status_code=500,
                error_code=-32603,
                error_message=f"Internal error: {str(e)}",
            )

    async def _execute_with_streaming_context(
        self,
        request: MethodRequest,
        context: StepflowContext,
        outgoing_queue: asyncio.Queue[MethodRequest | None],
    ) -> AsyncGenerator[str]:
        """Execute request with streaming context via SSE.

        This method runs component execution and bidirectional message processing
        concurrently to avoid deadlock scenarios where components need to make
        bidirectional requests (like put_blob) before completing.
        """

        async def execute_and_shutdown_queue():
            try:
                result = await self.server.handle_message(request, context)
                assert result is not None
                return result
            finally:
                # Signal end of queue by putting a sentinel value
                await outgoing_queue.put(None)

        # Start component execution as a background task
        execution_task = asyncio.create_task(execute_and_shutdown_queue())

        try:
            # Continuously process bidirectional messages while component runs.
            # It will close the stream when it is done.
            while True:
                message = await outgoing_queue.get()
                if message is None:
                    # Sentinel value indicates we're done processing events
                    break
                sse_data = msgspec.json.encode(message).decode("utf-8")
                yield f"data: {sse_data}\n\n"

            # At this point, the queue should be closed, which means the task should be
            # complete. Get the result.
            assert execution_task.done()
            result = await execution_task

            # Yield final response
            sse_data = msgspec.json.encode(result).decode("utf-8")
            yield f"data: {sse_data}\n\n"

        except Exception as e:
            # If we have an execution task, make sure it's cancelled
            if not execution_task.done():
                execution_task.cancel()

            # Yield error response
            yield self._create_sse_error_event(
                request_id=request.id,
                error_code=-32603,
                error_message=f"Request execution failed: {str(e)}",
            )

    async def run(self):
        """Start the HTTP server."""
        print(
            f"Starting Stepflow Streamable HTTP server on {self.host}:{self.port}",
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

    def component(self, *args, **kwargs):
        """Delegate component registration to the underlying server."""
        return self.server.component(*args, **kwargs)

    def langchain_component(self, *args, **kwargs):
        """Delegate langchain_component registration to the underlying server."""
        return self.server.langchain_component(*args, **kwargs)
