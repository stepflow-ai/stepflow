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

from __future__ import annotations

import asyncio
import json
import sys
import uuid
from typing import Any

import msgspec

from stepflow_sdk.context import StepflowContext
from stepflow_sdk.exceptions import (
    ComponentNotFoundError,
    ErrorCode,
    StepflowError,
    StepflowExecutionError,
)
from stepflow_sdk.generated_protocol import (
    ComponentExecuteParams,
    ComponentExecuteResult,
    ComponentInfo,
    ComponentInfoParams,
    ComponentInfoResult,
    ComponentListParams,
    ListComponentsResult,
    Method,
    MethodRequest,
)
from stepflow_sdk.message_decoder import MessageDecoder
from stepflow_sdk.server import StepflowServer

try:
    import uvicorn
    from fastapi import FastAPI, Request, Response
    from sse_starlette.sse import EventSourceResponse
except ImportError:
    print("Error: HTTP mode requires additional dependencies.", file=sys.stderr)
    print("Please install: pip install fastapi uvicorn sse-starlette", file=sys.stderr)
    sys.exit(1)


class StepflowSession:
    """Represents a single client session."""

    def __init__(self, session_id: str, server: StepflowServer):
        self.session_id = session_id
        self.server = server
        self.event_queue: asyncio.Queue = asyncio.Queue()
        self.message_decoder: MessageDecoder = MessageDecoder()
        self.context = StepflowContext(
            self.event_queue, self.message_decoder, session_id
        )
        self.connected = True


class StepflowHttpServer:
    def __init__(
        self,
        server: StepflowServer | None = None,
        host: str = "localhost",
        port: int = 8080,
    ):
        self.server = server or StepflowServer()
        self.host = host
        self.port = port
        self.app = FastAPI(title="StepFlow Component Server")
        self.sessions: dict[str, StepflowSession] = {}
        self._setup_routes()

    def _setup_routes(self):
        @self.app.get("/runtime/events")
        async def runtime_events():
            """SSE endpoint for bidirectional communication with MCP-style session negotiation"""  # noqa: E501
            return EventSourceResponse(self._event_stream())

        @self.app.post("/")
        async def handle_json_rpc(request: Request):
            """Handle JSON-RPC requests with session ID support"""
            request_id = None
            try:
                # Parse request body to extract ID for error responses
                body = await request.json()
                request_id = body.get("id")

                # Extract session ID from query parameters
                session_id = request.query_params.get("sessionId")
                if not session_id:
                    return Response(
                        content=json.dumps(
                            {
                                "jsonrpc": "2.0",
                                "id": request_id,
                                "error": {
                                    "code": -32600,
                                    "message": "Invalid Request: sessionId parameter required",  # noqa: E501
                                },
                            }
                        ),
                        status_code=400,
                        media_type="application/json",
                    )

                # Get or create session
                session = self.sessions.get(session_id)
                if not session:
                    return Response(
                        content=json.dumps(
                            {
                                "jsonrpc": "2.0",
                                "id": request_id,
                                "error": {
                                    "code": -32600,
                                    "message": "Invalid Request: session not found",
                                },
                            }
                        ),
                        status_code=400,
                        media_type="application/json",
                    )

                response = await self._handle_json_rpc(body, session)
                return response
            except Exception as e:
                return Response(
                    content=json.dumps(
                        {"jsonrpc": "2.0", "id": request_id, "error": str(e)}
                    ),
                    status_code=500,
                    media_type="application/json",
                )

    async def _event_stream(self):
        """Generate SSE events for runtime communication with MCP-style session negotiation"""  # noqa: E501
        # Create new session
        session_id = str(uuid.uuid4())
        session = StepflowSession(session_id, self.server)
        self.sessions[session_id] = session

        try:
            # Send endpoint event with session ID (MCP-style)
            endpoint_url = f"/?sessionId={session_id}"
            yield {"event": "endpoint", "data": json.dumps({"endpoint": endpoint_url})}

            # Process events from this session
            while session.connected:
                try:
                    # Wait for events from the session
                    event = await asyncio.wait_for(
                        session.event_queue.get(), timeout=30.0
                    )
                    yield {"event": "message", "data": json.dumps(event)}
                except TimeoutError:
                    # Send keep-alive
                    yield {"event": "keepalive", "data": ""}
                except Exception as e:
                    print(f"Error in SSE stream: {e}", file=sys.stderr)
                    break
        finally:
            # Clean up session
            session.connected = False
            if session_id in self.sessions:
                del self.sessions[session_id]

    async def _handle_json_rpc(
        self, body: dict[str, Any], session: StepflowSession
    ) -> dict[str, Any]:
        """Handle JSON-RPC requests with session support"""
        try:
            # Parse the JSON-RPC request
            if "method" in body:
                # This is a method request
                request = MethodRequest(**body)

                method = request.method
                if method == Method.components_list:
                    # Convert params to ComponentListParams
                    if isinstance(request.params, dict):
                        list_params = ComponentListParams(**request.params)
                    else:
                        raise StepflowExecutionError(
                            code=ErrorCode.INVALID_REQUEST,
                            message=f"Invalid params for components_list: {request.params}",
                        )
                    list_result = await self._handle_component_list(
                        list_params, session
                    )
                    return {
                        "jsonrpc": "2.0",
                        "id": request.id,
                        "result": msgspec.to_builtins(list_result),
                    }
                elif method == Method.components_info:
                    # Convert params to ComponentInfoParams
                    if isinstance(request.params, dict):
                        info_params = ComponentInfoParams(**request.params)
                    else:
                        raise StepflowExecutionError(
                            code=ErrorCode.INVALID_REQUEST,
                            message=f"Invalid params for components_info: {request.params}",
                        )
                    info_result = await self._handle_component_info(
                        info_params, session
                    )
                    return {
                        "jsonrpc": "2.0",
                        "id": request.id,
                        "result": msgspec.to_builtins(info_result),
                    }
                elif method == Method.components_execute:
                    # Convert params to ComponentExecuteParams
                    if isinstance(request.params, dict):
                        execute_params = ComponentExecuteParams(**request.params)
                    else:
                        raise StepflowExecutionError(
                            code=ErrorCode.INVALID_REQUEST,
                            message=f"Invalid params for components_execute: {request.params}",
                        )
                    execute_result = await self._handle_component_execute(
                        execute_params, session
                    )
                    return {
                        "jsonrpc": "2.0",
                        "id": request.id,
                        "result": msgspec.to_builtins(execute_result),
                    }
                else:
                    return {
                        "jsonrpc": "2.0",
                        "id": request.id,
                        "error": {"code": -32601, "message": "Method not found"},
                    }
            else:
                return {
                    "jsonrpc": "2.0",
                    "id": body.get("id"),
                    "error": {"code": -32600, "message": "Invalid Request"},
                }
        except StepflowError as e:
            return {
                "jsonrpc": "2.0",
                "id": body.get("id"),
                "error": e.to_json_rpc_error(),
            }
        except Exception as e:
            return {
                "jsonrpc": "2.0",
                "id": body.get("id"),
                "error": {"code": -32603, "message": "Internal error", "data": str(e)},
            }

    async def _handle_component_list(
        self, params: ComponentListParams, session: StepflowSession
    ) -> ListComponentsResult:
        """Handle component list request for a session."""
        if not session.server.is_initialized():
            # Auto-initialize for HTTP mode
            session.server.set_initialized(True)

        component_infos = []
        for name, component in session.server.get_components().items():
            component_url = f"/{session.server.get_protocol_prefix()}/{name}"
            component_infos.append(
                ComponentInfo(
                    component=component_url,
                    input_schema=component.input_schema(),
                    output_schema=component.output_schema(),
                    description=component.description,
                )
            )
        return ListComponentsResult(components=component_infos)

    async def _handle_component_info(
        self, params: ComponentInfoParams, session: StepflowSession
    ) -> ComponentInfoResult:
        """Handle component info request for a session."""
        if not session.server.is_initialized():
            # Auto-initialize for HTTP mode
            session.server.set_initialized(True)

        component_name = params.component.split("/")[-1]
        components = session.server.get_components()
        if component_name not in components:
            raise ComponentNotFoundError(f"Component '{component_name}' not found")

        component = components[component_name]
        info = ComponentInfo(
            component=params.component,
            input_schema=component.input_schema(),
            output_schema=component.output_schema(),
            description=component.description,
        )
        return ComponentInfoResult(info=info)

    async def _handle_component_execute(
        self, params: ComponentExecuteParams, session: StepflowSession
    ) -> ComponentExecuteResult:
        """Handle component execute request for a session."""
        if not session.server.is_initialized():
            # Auto-initialize for HTTP mode
            session.server.set_initialized(True)

        component_name = params.component.split("/")[-1]
        components = session.server.get_components()
        if component_name not in components:
            raise ComponentNotFoundError(f"Component '{component_name}' not found")

        component = components[component_name]

        try:
            # The input is already a dictionary (JSON object)
            input_data = msgspec.json.encode(params.input)

            # Deserialize input using the component's input type
            input_value: Any = msgspec.json.decode(
                input_data, type=component.input_type
            )

            # Execute the component with session context
            if component.function.__code__.co_argcount == 2:
                # Function expects context as second parameter
                result = component.function(input_value, session.context)
            else:
                # Function only expects input
                result = component.function(input_value)

            # Serialize the result back to JSON
            output_data = msgspec.json.encode(result)
            output = msgspec.json.decode(output_data)

            return ComponentExecuteResult(output=output)
        except Exception as e:
            raise StepflowExecutionError(f"Component execution failed: {str(e)}") from e

    async def run(self):
        """Start the HTTP server"""
        print(
            f"Starting StepFlow HTTP server on {self.host}:{self.port}", file=sys.stderr
        )

        # Initialize the base server
        self.server.set_initialized(True)

        # Start the HTTP server
        config = uvicorn.Config(
            app=self.app, host=self.host, port=self.port, log_level="info"
        )
        server = uvicorn.Server(config)
        await server.serve()
