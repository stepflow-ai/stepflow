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
import sys
from collections.abc import Callable

import msgspec

from stepflow_py.context import StepflowContext
from stepflow_py.generated_protocol import (
    Message,
    MethodRequest,
    Notification,
)
from stepflow_py.message_decoder import MessageDecoder
from stepflow_py.server import ComponentEntry, StepflowServer, _handle_exception


class StepflowStdioServer:
    """STDIO transport wrapper for StepflowServer."""

    def __init__(self, server: StepflowServer | None = None):
        self._server = server or StepflowServer()
        self._incoming_queue: asyncio.Queue = asyncio.Queue()
        self._outgoing_queue: asyncio.Queue = asyncio.Queue()
        self._message_decoder: MessageDecoder[asyncio.Future[Message]] = (
            MessageDecoder()
        )
        self._context: StepflowContext = StepflowContext(
            self._outgoing_queue, self._message_decoder, session_id=None
        )

    def component(
        self,
        func: Callable | None = None,
        *,
        name: str | None = None,
        description: str | None = None,
    ):
        """Delegate component registration to the underlying server."""
        return self._server.component(func, name=name, description=description)

    def get_component(self, component_path: str) -> ComponentEntry | None:
        """Get a registered component by path."""
        return self._server.get_component(component_path)

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
            if isinstance(message, MethodRequest | Notification):
                if self._server.requires_context(message):
                    response = await self._server.handle_message(message, self._context)
                else:
                    response = await self._server.handle_message(message)
            else:
                # This shouldn't happen for incoming messages, but handle gracefully
                print(f"Unexpected message type: {type(message)}", file=sys.stderr)
                return

            # Encode and write response
            print(f"Sending response: {response} to {message}", file=sys.stderr)
            response_bytes = msgspec.json.encode(response) + b"\n"
            sys.stdout.buffer.write(response_bytes)
            sys.stdout.buffer.flush()
        except Exception as e:
            print(f"Error in _handle_incoming_message: {e}", file=sys.stderr)
            if request_id is not None:
                error_response = _handle_exception(e, id=request_id)
                sys.stdout.buffer.write(msgspec.json.encode(error_response) + b"\n")
                sys.stdout.buffer.flush()
            else:
                # If we can't identify the request, we can't send a proper error
                # response so just log the error
                print(
                    f"Failed to handle message without request ID: {e}",
                    file=sys.stderr,
                )
            return

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

    async def start(
        self,
        stdin: asyncio.StreamReader | None = None,
        stdout: asyncio.StreamWriter | None = None,
    ):
        """Start the server and begin processing messages.

        Args:
            stdin: Optional StreamReader to read from (defaults to sys.stdin)
            stdout: Optional StreamWriter to write to (defaults to sys.stdout)
        """
        # Set up reader - use provided or create from sys.stdin
        if stdin is not None:
            reader = stdin
        else:
            loop = asyncio.get_event_loop()
            reader = asyncio.StreamReader()
            protocol = asyncio.StreamReaderProtocol(reader)
            await loop.connect_read_pipe(lambda: protocol, sys.stdin)

        # Set up writer - use provided or create from sys.stdout
        if stdout is not None:
            writer = stdout
        else:
            loop = asyncio.get_event_loop()
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

    def run(
        self,
        stdin: asyncio.StreamReader | None = None,
        stdout: asyncio.StreamWriter | None = None,
    ):
        """Run the server in the main thread.

        Args:
            stdin: Optional StreamReader to read from (defaults to sys.stdin)
            stdout: Optional StreamWriter to write to (defaults to sys.stdout)
        """
        asyncio.run(self.start(stdin, stdout))
