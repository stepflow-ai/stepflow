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

from typing import Any, Callable, Dict, Optional, Type, TypeVar, Union
from functools import wraps
import inspect
import json
import sys
import asyncio
from dataclasses import dataclass
from urllib.parse import urlparse

import msgspec
from msgspec import UNSET
from stepflow_sdk.exceptions import (
    ComponentNotFoundError,
    InputValidationError,
    ServerNotInitializedError,
    StepflowError,
    StepflowExecutionError,
    StepflowProtocolError,
)
from stepflow_sdk.generated_protocol import (
    RequestId,
    Message,
    MethodRequest,
    MethodResponse,
    MethodSuccess,
    MethodError,
    Notification,
    InitializeParams,
    InitializeResult,
    ComponentInfoParams,
    ComponentInfoResult,
    ComponentExecuteParams,
    ComponentExecuteResult,
    ComponentInfo,
    Component,
    ListComponentsResult,
    Method,
)
from stepflow_sdk.message_decoder import MessageDecoder
from stepflow_sdk.context import StepflowContext


@dataclass
class ComponentEntry:
    name: str
    function: Callable
    input_type: Type
    output_type: Type
    description: Optional[str] = None

    def input_schema(self):
        return msgspec.json.schema(self.input_type)

    def output_schema(self):
        return msgspec.json.schema(self.output_type)


def _handle_exception(e: Exception, id: RequestId | None) -> MethodError:
    """Convert any exception to a proper JSON-RPC error response."""
    if not isinstance(e, StepflowError):
        e = StepflowExecutionError(f"Unexpected error: {str(e)}")

    return MethodError(id=id, error=e.to_json_rpc_error())


class StepflowStdioServer:
    def __init__(self, default_protocol_prefix: str = "python"):
        self._components: Dict[str, ComponentEntry] = {}
        self._initialized = False
        self._protocol_prefix: str = default_protocol_prefix
        self._incoming_queue: asyncio.Queue = asyncio.Queue()
        self._outgoing_queue: asyncio.Queue = asyncio.Queue()
        self._message_decoder: MessageDecoder[asyncio.Future] = MessageDecoder()
        self._context: StepflowContext = StepflowContext(
            self._outgoing_queue, self._message_decoder
        )

    def component(
        self,
        func: Optional[Callable] = None,
        *,
        name: Optional[str] = None,
        description: Optional[str] = None,
    ):
        """
        Decorator to register a component function.

        Args:
            func: The function to register (provided by the decorator)
            name: Optional name for the component. If not provided, uses the function name
            description: Optional description. If not provided, uses the function's docstring
        """

        def decorator(f: Callable) -> Callable:
            component_name = name or f.__name__

            # Get input and output types from type hints
            sig = inspect.signature(f)
            params = list(sig.parameters.items())

            # Check if function expects context as second parameter
            expects_context = False
            if len(params) >= 2 and params[1][1].name == "context":
                expects_context = True
                input_type = params[0][1].annotation
            else:
                # TODO: Verify input signature.
                input_type = params[0][1].annotation

            return_type = sig.return_annotation

            # Extract description from parameter or docstring
            component_description = description or (
                f.__doc__.strip() if f.__doc__ else None
            )

            self._components[component_name] = ComponentEntry(
                name=component_name,
                function=f,
                input_type=input_type,
                output_type=return_type,
                description=component_description,
            )

            # Store whether function expects context
            f._expects_context = expects_context

            @wraps(f)
            def wrapper(*args, **kwargs):
                return f(*args, **kwargs)

            return wrapper

        if func is None:
            return decorator
        return decorator(func)

    def get_component(self, component_url: str) -> ComponentEntry | None:
        """Get a registered component by name."""
        parse_result = urlparse(component_url)
        component_name = parse_result.netloc
        if parse_result.path:
            component_name += "/" + parse_result.path
        return self._components.get(component_name)

    async def _handle_method_request(self, request: MethodRequest) -> MethodResponse:
        """Handle a method request and return a response."""

        id = request.id
        match request.method:
            case Method.initialize:
                init_request = msgspec.json.decode(
                    msgspec.json.encode(request.params), type=InitializeParams
                )
                self._protocol_prefix = init_request.protocol_prefix
                return MethodSuccess(
                    id=id,
                    result=InitializeResult(server_protocol_version=1),
                )
            case Method.components_info:
                component_request = msgspec.json.decode(
                    msgspec.json.encode(request.params), type=ComponentInfoParams
                )
                component = self.get_component(component_request.component)
                if not component:
                    raise ComponentNotFoundError(component_request.component)
                return MethodSuccess(
                    id=id,
                    result=ComponentInfoResult(
                        info=ComponentInfo(
                            component=component_request.component,
                            input_schema=component.input_schema(),
                            output_schema=component.output_schema(),
                            description=component.description,
                        )
                    ),
                )
            case Method.components_execute:
                execute_request = msgspec.json.decode(
                    msgspec.json.encode(request.params), type=ComponentExecuteParams
                )
                component = self.get_component(execute_request.component)
                if not component:
                    raise ComponentNotFoundError(execute_request.component)
                # Parse input parameters into the expected type
                try:
                    # execute_request.input is a Value, decode to the expected component type
                    input = msgspec.convert(
                        execute_request.input, type=component.input_type
                    )
                except (msgspec.DecodeError, msgspec.ValidationError) as e:
                    raise InputValidationError(
                        f"Input validation failed: {str(e)}",
                        input_data=msgspec.json.encode(execute_request.input),
                    )

                # Execute component with or without context
                import asyncio
                import inspect

                if (
                    hasattr(component.function, "_expects_context")
                    and component.function._expects_context
                ):
                    if inspect.iscoroutinefunction(component.function):
                        output = await component.function(input, self._context)
                    else:
                        output = component.function(input, self._context)
                else:
                    if inspect.iscoroutinefunction(component.function):
                        output = await component.function(input)
                    else:
                        output = component.function(input)

                return MethodSuccess(
                    id=id,
                    result=ComponentExecuteResult(output=output),
                )
            case Method.components_list:
                # Return component info objects
                component_infos = []
                for name, component in self._components.items():
                    component_url = f"{self._protocol_prefix}://{name}"
                    component_infos.append(
                        ComponentInfo(
                            component=component_url,
                            input_schema=component.input_schema(),
                            output_schema=component.output_schema(),
                            description=component.description,
                        )
                    )
                return MethodSuccess(
                    id=id,
                    result=ListComponentsResult(components=component_infos),
                )
            case _:
                raise StepflowProtocolError(f"Unknown method '{request.method}'")

    async def _handle_notification(self, notification: Notification):
        """Handle a notification and return a response."""
        match notification.method:
            case Method.initialized:
                self._initialized = True
            case _:
                print(
                    f"Received unknown notification {notification.method}",
                    file=sys.stderr,
                )

    async def _handle_incoming_message(self, request_bytes: bytes):
        """Handle an incoming message in a separate task."""

        request_id = None
        try:
            # Decode message using message decoder
            message, future = self._message_decoder.decode(request_bytes)
            print(f"Received message: {message}", file=sys.stderr)

            # Extract request ID for error handling
            request_id = getattr(message, "id", None)

            # If this was a response to one of our outgoing requests, the future
            # has already been resolved by the MessageDecoder, so we're done
            if future is not None:
                future.set_result(message)
                print(f"Resolved pending request {request_id}", file=sys.stderr)
                return

            # Otherwise, this is an incoming request that we need to handle
            response = await self._handle_message(message)

            # Encode and write response
            if response is not None:
                print(f"Sending response: {response} to {message}", file=sys.stderr)
                response_bytes = msgspec.json.encode(response) + b"\n"
                sys.stdout.buffer.write(response_bytes)
                sys.stdout.buffer.flush()
            else:
                print(f"No response for message: {message}", file=sys.stderr)
        except Exception as e:
            print(f"Error in _handle_incoming_message: {e}", file=sys.stderr)
            error_response = _handle_exception(e, id=request_id)
            sys.stdout.buffer.write(msgspec.json.encode(error_response) + b"\n")
            sys.stdout.buffer.flush()
            return

    async def _handle_message(self, message: Message) -> MethodResponse | None:
        """Handle an incoming message and return a response."""
        if isinstance(message, MethodRequest):
            if not self._initialized and message.method != Method.initialize:
                raise ServerNotInitializedError()
            return await self._handle_method_request(message)
        elif isinstance(message, (MethodSuccess, MethodError)):
            # Response messages should be handled by the MessageDecoder in _handle_incoming_message
            # and should not reach this point
            raise StepflowProtocolError("Unexpected response message in _handle_message")
        elif isinstance(message, Notification):
            if message.method == Method.initialized:
                self._initialized = True
            await self._handle_notification(message)
            return None
        else:
            print(f"Received unknown message type: {message}", file=sys.stderr)
            return None

    async def _process_messages(self, writer: asyncio.StreamWriter):
        """Process messages from both incoming and outgoing queues asynchronously."""
        print("Starting process messages", file=sys.stderr)
        while True:
            # Wait for either incoming or outgoing messages
            try:
                # Use asyncio.wait with FIRST_COMPLETED to handle both queues
                incoming_task = asyncio.create_task(self._incoming_queue.get())
                outgoing_task = asyncio.create_task(self._outgoing_queue.get())

                done, pending = await asyncio.wait(
                    [incoming_task, outgoing_task], return_when=asyncio.FIRST_COMPLETED
                )

                # Cancel pending tasks
                for task in pending:
                    task.cancel()

                # Handle completed task
                for task in done:
                    if task == incoming_task:
                        # Handle incoming message
                        request_bytes = task.result()
                        asyncio.create_task(
                            self._handle_incoming_message(request_bytes)
                        )
                    elif task == outgoing_task:
                        # Handle outgoing message
                        outgoing_message = task.result()
                        await self._send_outgoing_message(outgoing_message, writer)

            except Exception as e:
                print(f"Error in message processing loop: {e}", file=sys.stderr)

    async def _send_outgoing_message(self, message_data, writer: asyncio.StreamWriter):
        """Send an outgoing message to the runtime."""
        try:
            message_bytes = msgspec.json.encode(message_data) + b"\n"
            writer.write(message_bytes)
            await writer.drain()
            print(f"Sent outgoing message: {message_data}", file=sys.stderr)
        except Exception as e:
            print(f"Error sending outgoing message: {e}", file=sys.stderr)

    # _handle_response method removed - MessageDecoder now handles response processing

    async def start(self):
        """Start the server and begin processing messages."""
        # Set up unbuffered binary IO
        # Create async streams for stdin/stdout
        loop = asyncio.get_event_loop()
        reader = asyncio.StreamReader()
        protocol = asyncio.StreamReaderProtocol(reader)
        await loop.connect_read_pipe(lambda: protocol, sys.stdin)

        writer_transport, writer_protocol = await loop.connect_write_pipe(
            asyncio.streams.FlowControlMixin, sys.stdout
        )
        writer = asyncio.StreamWriter(writer_transport, writer_protocol, None, loop)

        # Start async processing loop
        process_task = asyncio.create_task(self._process_messages(writer))

        # Read messages from stdin and add to queue
        try:
            while True:
                line = await reader.readline()
                if not line:
                    print("Empty line received. Exiting", file=sys.stderr)
                    break
                await self._incoming_queue.put(line)
        except KeyboardInterrupt:
            pass
        finally:
            # Clean up
            process_task.cancel()

    def run(self):
        """Run the server in the main thread."""
        asyncio.run(self.start())
