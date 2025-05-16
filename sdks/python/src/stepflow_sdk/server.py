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
from stepflow_sdk.transport import Incoming, MethodResponse
from stepflow_sdk.protocol import ComponentInfoRequest, ComponentInfoResponse, ComponentExecuteRequest, ComponentExecuteResponse

@dataclass
class ComponentEntry:
    name: str
    function: Callable
    input_type: Type[msgspec.Struct]
    output_type: Type[msgspec.Struct]

    def input_schema(self):
        return msgspec.json.schema(self.input_type)

    def output_schema(self):
        return msgspec.json.schema(self.output_type)

class StepflowStdioServer:
    def __init__(self):
        self._components: Dict[str, ComponentEntry] = {}
        self._initialized = False

    def component(self, func: Optional[Callable] = None, *, name: Optional[str] = None):
        """
        Decorator to register a component function.
        
        Args:
            func: The function to register (provided by the decorator)
            name: Optional name for the component. If not provided, uses the function name
        """
        def decorator(f: Callable) -> Callable:
            component_name = name or f.__name__
            
            # Get input and output types from type hints
            sig = inspect.signature(f)
            # TODO: Verify input signature.
            input_type = sig.parameters[list(sig.parameters.keys())[0]].annotation
            return_type = sig.return_annotation
            
            # Validate types
            if not (issubclass(input_type, msgspec.Struct) and issubclass(return_type, msgspec.Struct)):
                raise TypeError(f"Component {component_name} must have input and output types that inherit from msgspec.Struct")

            self._components[component_name] = ComponentEntry(
                name=component_name,
                function=f,
                input_type=input_type,
                output_type=return_type
            )

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

    async def _handle_message(self, request: Incoming) -> MethodResponse | None:
        """Handle an incoming method request and return a response."""
        if request.id is None and request.method == "initialized":
            self._initialized = True
            return None

        if not self._initialized and request.method != "initialize":
            return MethodResponse(
                id=request.id,
                error={
                    "code": -32002,
                    "message": "Server not initialized",
                    "data": None
                }
            )

        id = request.id
        if request.method == "initialize":
            return MethodResponse(
                id=request.id,
                result={"server_protocol_version": 1}
            )
        elif request.method == "component_info":
            request = msgspec.json.decode(request.params, type=ComponentInfoRequest)            
            component = self.get_component(request.component)
            if not component:
                return MethodResponse(
                    id=id,
                    error={
                        "code": -32601,
                        "message": f"Component {request.component} not found",
                        "data": None
                    }
                )
            return MethodResponse(
                id=id,
                result=ComponentInfoResponse(
                    input_schema=component.input_schema(),
                    output_schema=component.output_schema(),
                )
            )
        elif request.method == "component_execute":
            request = msgspec.json.decode(request.params, type=ComponentExecuteRequest)
            component = self.get_component(request.component)
            if not component:
                return MethodResponse(
                    id=id,
                    error={
                        "code": -32601,
                        "message": f"Component {request.component} not found",
                        "data": None
                    }
                )
            try:
                # Parse input parameters into the expected type
                # Execute component
                input = msgspec.json.decode(request.input, type=component.input_type)
                output = component.function(input)
                return MethodResponse(
                    id=id,
                    result=ComponentExecuteResponse(
                        output=msgspec.Raw(msgspec.json.encode(output)),
                    ),
                )
            except Exception as e:
                return MethodResponse(
                    id=id,
                    error={
                        "code": -32000,
                        "message": str(e),
                        "data": None
                    }
                )
        elif request.method == "component_list":
            return MethodResponse(
                id=request.id,
                result={"components": list(self._components.keys())}
            )
        else:
            return MethodResponse(
                id=request.id,
                error={
                    "code": -32601,
                    "message": f"Unknown method: {request.method}",
                    "data": None
                }
            )

    async def _process_messages(self, message_queue: asyncio.Queue):
        """Process messages from the queue asynchronously."""
        print("Starting process messages", file=sys.stderr)
        while True:
            request_bytes = await message_queue.get()
            try:
                # Decode the request
                # print(f"Received request_bytes: {request_bytes}", file=sys.stderr)
                request = msgspec.json.decode(request_bytes, type=Incoming)
                print(f"Received request: {request}", file=sys.stderr)
                
                # Handle the request and get response
                response = await self._handle_message(request)
                
                # Encode and write response
                if response is not None:
                    print(f"Sending response: {response} to {request}", file=sys.stderr)
                    response_bytes = msgspec.json.encode(response) + b"\n"
                    sys.stdout.buffer.write(response_bytes)
                    sys.stdout.buffer.flush()
                
            except Exception as e:
                print(f"Error: {e}", file=sys.stderr)
                # Send error response
                error_response = MethodResponse(
                    error={
                        "code": -32000,
                        "message": str(e),
                        "data": None
                    }
                )
                sys.stdout.buffer.write(msgspec.json.encode(error_response) + b"\n")
                sys.stdout.buffer.flush()

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

        # Create message queue
        message_queue: asyncio.Queue = asyncio.Queue()
        
        # Start async processing loop
        process_task = asyncio.create_task(self._process_messages(message_queue))

        # Read messages from stdin and add to queue
        try:
            while True:
                line = await reader.readline()
                if not line:
                    print("Empty line received. Exiting", file=sys.stderr)
                    break
                await message_queue.put(line)
        except KeyboardInterrupt:
            pass
        finally:
            # Clean up
            process_task.cancel()

    def run(self):
        """Run the server in the main thread."""
        asyncio.run(self.start())