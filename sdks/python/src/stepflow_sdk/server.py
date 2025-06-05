from typing import Any, Callable, Dict, Optional, Type, TypeVar, Union
from functools import wraps
import inspect
import json
import sys
import asyncio
from dataclasses import dataclass
from uuid import UUID
from urllib.parse import urlparse

import msgspec
from stepflow_sdk.transport import Message
from stepflow_sdk.protocol import InitializeRequest, ComponentInfoRequest, ComponentInfoResponse, ComponentExecuteRequest, ComponentExecuteResponse
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

class StepflowStdioServer:
    def __init__(self):
        self._components: Dict[str, ComponentEntry] = {}
        self._initialized = False
        self._protocol_prefix: str = ""
        self._incoming_queue: asyncio.Queue = asyncio.Queue()
        self._outgoing_queue: asyncio.Queue = asyncio.Queue()
        self._pending_requests: Dict[UUID, asyncio.Future] = {}
        self._context: StepflowContext = StepflowContext(self._outgoing_queue, self._pending_requests)

    def component(self, func: Optional[Callable] = None, *, name: Optional[str] = None, description: Optional[str] = None):
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
            component_description = description or (f.__doc__.strip() if f.__doc__ else None)
            
            self._components[component_name] = ComponentEntry(
                name=component_name,
                function=f,
                input_type=input_type,
                output_type=return_type,
                description=component_description
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

    async def _handle_method_request(self, request: Message) -> Message | None:
        """Handle a method request and return a response."""

        id = request.id
        match request.method:
            case "initialize":
                init_request = msgspec.json.decode(request.params, type=InitializeRequest)
                self._protocol_prefix = init_request.protocol_prefix
                return Message(
                    id=id,
                    result={"server_protocol_version": 1}
                )
            case "component_info":
                request = msgspec.json.decode(request.params, type=ComponentInfoRequest)            
                component = self.get_component(request.component)
                if not component:
                    return Message(
                        id=id,
                        error={
                            "code": -32601,
                            "message": f"Component {request.component} not found",
                            "data": None
                        }
                    )
                return Message(
                    id=id,
                    result=ComponentInfoResponse(
                        input_schema=component.input_schema(),
                        output_schema=component.output_schema(),
                        description=component.description,
                    )
                )
            case "component_execute":
                request = msgspec.json.decode(request.params, type=ComponentExecuteRequest)
                component = self.get_component(request.component)
                if not component:
                    return Message(
                        id=id,
                        error={
                            "code": -32601,
                            "message": f"Component {request.component} not found",
                            "data": None
                        }
                    )
                try:
                    # Parse input parameters into the expected type
                    input = msgspec.json.decode(request.input, type=component.input_type)
                    
                    # Execute component with or without context
                    import asyncio
                    import inspect
                    
                    if hasattr(component.function, '_expects_context') and component.function._expects_context:
                        if inspect.iscoroutinefunction(component.function):
                            output = await component.function(input, self._context)
                        else:
                            output = component.function(input, self._context)
                    else:
                        if inspect.iscoroutinefunction(component.function):
                            output = await component.function(input)
                        else:
                            output = component.function(input)
                        
                    return Message(
                        id=id,
                        result=ComponentExecuteResponse(output=output),
                    )
                except Exception as e:
                    return Message(
                        id=id,
                        error={
                            "code": -32000,
                            "message": str(e),
                            "data": None
                        }
                    )
            case "list_components":
                # Return component URLs that match the expected Component format
                component_urls = [f"{self._protocol_prefix}://{name}" for name in self._components.keys()]
                return Message(
                    id=id,
                    result={"components": component_urls}
                )
            case _:
                print(f"Received unknown method request {request.method}", file=sys.stderr)
                return Message(
                    id=id,
                    error={
                        "code": -32601,
                        "message": f"Unknown method {request.method}",
                        "data": None
                    }
                )

    async def _handle_notification(self, request: Message):
        """Handle a notification and return a response."""
        match request.method:
            case "initialized":
                self._initialized = True
            case _:
                print(f"Received unknown notification {request.method}", file=sys.stderr)

    async def _handle_message(self, request: Message) -> Message | None:
        """Handle an incoming method request and return a response."""
        if request.id is None and request.method == "initialized":
            self._initialized = True
            return None

        if not self._initialized and request.method != "initialize":
            return Message(
                id=request.id,
                error={
                    "code": -32002,
                    "message": "Server not initialized",
                    "data": None
                }
            )

        if request.id is not None and request.method is not None:
            # Handle a method request.
            return await self._handle_method_request(request)
        elif request.id is not None:
            # Handle a method response.
            self._handle_response(request)
            return None
        elif request.method is not None:
            return await self._handle_notification(request)
        else:
            # Handle a request with no id or method.
            print(f"Received request with no id or method {request}", file=sys.stderr)
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
                    [incoming_task, outgoing_task], 
                    return_when=asyncio.FIRST_COMPLETED
                )
                
                # Cancel pending tasks
                for task in pending:
                    task.cancel()
                
                # Handle completed task
                for task in done:
                    if task == incoming_task:
                        # Handle incoming message
                        request_bytes = task.result()
                        asyncio.create_task(self._handle_incoming_message(request_bytes))
                    elif task == outgoing_task:
                        # Handle outgoing message
                        outgoing_message = task.result()
                        await self._send_outgoing_message(outgoing_message, writer)
                        
            except Exception as e:
                print(f"Error in message processing loop: {e}", file=sys.stderr)
    
    async def _handle_incoming_message(self, request_bytes: bytes):
        """Handle an incoming message in a separate task."""

        request = None
        try:
            # Decode the request
            request = msgspec.json.decode(request_bytes, type=Message)
            print(f"Received request: {request}", file=sys.stderr)
        except Exception as e:
            print(f"Error decoding incoming message: {request_bytes}", e, file=sys.stderr)
            return
        
        response = None
        try:            
            # Handle the request and get response (reusing existing logic)
            response = await self._handle_message(request)
        except Exception as e:
            print(f"Error handling incoming message: {request}", e, file=sys.stderr)
            # Send error response if we can determine an ID
            try:
                # TODO: Special kind of exception to raise with explicit code?
                if hasattr(request, 'id') and request.id:
                    error_response = Message(
                        id=request.id,
                        error={
                            "code": -32000,
                            "message": str(e),
                            "data": None
                        }
                    )
                    sys.stdout.buffer.write(msgspec.json.encode(error_response) + b"\n")
                    sys.stdout.buffer.flush()
            except:
                pass  # Failed to send error response
            return

        try:
            # Encode and write response
            if response is not None:
                print(f"Sending response: {response} to {request}", file=sys.stderr)
                response_bytes = msgspec.json.encode(response) + b"\n"
                sys.stdout.buffer.write(response_bytes)
                sys.stdout.buffer.flush()
                
        except Exception as e:
            print(f"Error sending response: {response}", e, file=sys.stderr)
            # Send error response if we can determine an ID
            try:
                # TODO: Special kind of exception to raise with explicit code?
                if hasattr(request, 'id') and request.id:
                    error_response = Message(
                        id=request.id,
                        error={
                            "code": -32000,
                            "message": str(e),
                            "data": None
                        }
                    )
                    sys.stdout.buffer.write(msgspec.json.encode(error_response) + b"\n")
                    sys.stdout.buffer.flush()
            except:
                pass  # Failed to send error response
            return

    
    async def _send_outgoing_message(self, message_data, writer: asyncio.StreamWriter):
        """Send an outgoing message to the runtime."""
        try:
            message_bytes = msgspec.json.encode(message_data) + b"\n"
            writer.write(message_bytes)
            await writer.drain()
            print(f"Sent outgoing message: {message_data}", file=sys.stderr)
        except Exception as e:
            print(f"Error sending outgoing message: {e}", file=sys.stderr)
    
    def _handle_response(self, message: Message):
        """Handle a response message from the runtime."""
        if message.id and message.id in self._pending_requests:
            print(f"Handling response to {message.id}: {message}", file=sys.stderr)
            future = self._pending_requests.pop(message.id)
            
            if message.error:
                # Create exception from error
                error_msg = f"Runtime error ({message.error.code}): {message.error.message}"
                future.set_exception(RuntimeError(error_msg))
            elif message.result:
                # Decode result
                result = msgspec.json.decode(message.result)
                future.set_result(result)
            else:
                future.set_exception(RuntimeError("Invalid response: no result or error"))

    async def start(self):
        """Start the server and begin processing messages."""
        # Set up unbuffered binary IO
        # Create async streams for stdin/stdout
        loop = asyncio.get_event_loop()
        reader = asyncio.StreamReader()
        protocol = asyncio.StreamReaderProtocol(reader)
        await loop.connect_read_pipe(lambda: protocol, sys.stdin)

        writer_transport, writer_protocol = await loop.connect_write_pipe(
            asyncio.streams.FlowControlMixin, 
            sys.stdout
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