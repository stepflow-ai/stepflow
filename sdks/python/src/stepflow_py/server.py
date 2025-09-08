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

from __future__ import annotations

import inspect
import sys
import traceback
from collections.abc import Callable
from dataclasses import dataclass
from functools import wraps
from typing import Any, assert_never

import msgspec

from stepflow_py.context import StepflowContext
from stepflow_py.exceptions import (
    ComponentNotFoundError,
    SkipStep,
    StepflowError,
    StepflowExecutionError,
    StepflowProtocolError,
)
from stepflow_py.generated_protocol import (
    ComponentExecuteParams,
    ComponentExecuteResult,
    ComponentInfo,
    ComponentInfoParams,
    ComponentInfoResult,
    ComponentListParams,
    Error,
    FlowResultSkipped,
    InitializeResult,
    ListComponentsResult,
    Message,
    Method,
    MethodError,
    MethodRequest,
    MethodResponse,
    MethodSuccess,
    Notification,
    RequestId,
)
from stepflow_py.udf import udf

# Check if LangChain is available
try:
    from stepflow_py.langchain_components import register_langchain_components

    _HAS_LANGCHAIN = True
except ImportError:
    _HAS_LANGCHAIN = False


@dataclass
class ComponentEntry:
    name: str
    function: Callable
    input_type: type
    output_type: type
    description: str | None = None

    def input_schema(self):
        return msgspec.json.schema(self.input_type)

    def output_schema(self):
        return msgspec.json.schema(self.output_type)


def _handle_exception(e: Exception, id: RequestId) -> MethodError:
    """Convert any exception to a proper JSON-RPC error response."""
    if not isinstance(e, StepflowError):
        e = StepflowExecutionError(f"Unexpected error: {str(e)}")

    error_dict = e.to_json_rpc_error()
    error_obj = Error(
        code=error_dict["code"],
        message=error_dict["message"],
        data=error_dict.get("data"),
    )

    return MethodError(id=id, error=error_obj)


class StepflowServer:
    """Unified Stepflow server with component registry and transport methods."""

    def __init__(self, include_builtins: bool = True):
        self._components: dict[str, ComponentEntry] = {}
        self._initialized = False

        # Add LangChain registry functionality if available
        if _HAS_LANGCHAIN:
            register_langchain_components(self)

        if include_builtins:
            # Register the UDF component
            self.component(udf)

    def is_initialized(self) -> bool:
        """Check if the server is initialized."""
        return self._initialized

    def set_initialized(self, initialized: bool):
        """Set the initialization state."""
        self._initialized = initialized

    def component(
        self,
        func: Callable | None = None,
        *,
        name: str | None = None,
        description: str | None = None,
    ):
        """Decorator to register a component function.

        Args:
            func: The function to register (provided by the decorator)
            name: Optional name for the component. If not provided, uses the function
                name
            description: Optional description. If not provided, uses the function's
                docstring
        """

        def decorator(f: Callable) -> Callable:
            component_name = name or f.__name__
            if not component_name.startswith("/"):
                component_name = f"/{component_name}"

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
            f._expects_context = expects_context  # type: ignore[attr-defined]

            if inspect.iscoroutinefunction(f):
                # If function is async, wrap it to ensure it can be called
                # with or without context
                @wraps(f)
                async def wrapper(*args, **kwargs):
                    return await f(*args, **kwargs)

            else:

                @wraps(f)
                def wrapper(*args, **kwargs):
                    return f(*args, **kwargs)

            return wrapper

        if func is None:
            return decorator
        return decorator(func)

    def get_component(self, component_path: str) -> ComponentEntry | None:
        """Get a registered component by path."""
        return self._components.get(component_path)

    def get_components(self) -> dict[str, ComponentEntry]:
        """Get all registered components."""
        return self._components

    def requires_context(self, message: Message) -> bool:
        """Check if a message requires bidirectional communication context.

        Args:
            message: Parsed JSON-RPC message

        Returns:
            True if message requires StepflowContext for bidirectional communication
        """
        if isinstance(message, MethodRequest):
            # Only component execution may require context
            if message.method == Method.components_execute:
                try:
                    # Parse the component name from the request
                    assert isinstance(message.params, ComponentExecuteParams)
                    component = self._components.get(message.params.component)
                    if component is None:
                        # Component not found - doesn't require context (errors later)
                        return False

                    # Check if component function expects context parameter
                    return (
                        hasattr(component.function, "_expects_context")
                        and component.function._expects_context
                    )
                except Exception:
                    # If we can't parse the request, assume no context needed
                    return False

            # All other methods don't require context
            return False

        # Notifications and responses don't require context
        return False

    async def handle_message(
        self,
        message: MethodRequest | Notification,
        context: StepflowContext | None = None,
    ) -> MethodResponse | None:
        """Central message handler for all JSON-RPC protocol methods.

        This method handles the core Stepflow protocol logic and should be called
        by transport servers (HTTP, STDIO) after they parse incoming messages.

        Args:
            message: Parsed JSON-RPC message
            context: Context for bidirectional communication. MUST be provided if
                    requires_context(message) returns True.

        Returns:
            MethodResponse (either MethodSuccess or MethodError)
        """
        # Validate context requirement
        if self.requires_context(message) and context is None:
            raise StepflowProtocolError("Message requires context but none provided")

        if isinstance(message, MethodRequest):
            return await self._handle_request(message, context)
        elif isinstance(message, Notification):
            await self._handle_notification(message, context)
            return None
        else:
            assert_never("Unexpected message type in handle_message")

    async def _handle_request(
        self, request: MethodRequest, context: StepflowContext | None = None
    ) -> MethodResponse:
        """Handle a JSON-RPC method request."""
        try:
            # Route known methods
            if request.method == Method.initialize:
                return await self._handle_initialize(request)
            elif request.method == Method.components_list:
                return await self._handle_component_list(request)
            elif request.method == Method.components_info:
                return await self._handle_component_info(request)
            elif request.method == Method.components_execute:
                return await self._handle_component_execute(request, context)
            else:
                return MethodError(
                    jsonrpc="2.0",
                    id=request.id,
                    error=Error(
                        code=-32601,  # Method not found
                        message=f"Method not found: {request.method}",
                        data=None,
                    ),
                )
        except Exception as e:
            return _handle_exception(e, request.id)

    async def _handle_notification(
        self, notification: Notification, context: StepflowContext | None = None
    ):
        """Handle a JSON-RPC notification."""
        # For now, don't process notifications, could handle 'initialized' here
        # Return a success response (though notifications don't expect responses)
        # Create a dummy InitializeResult for notification response
        # (notifications don't typically expect responses)
        assert notification.method == Method.initialized, (
            "Only '{Method.initialized.value}' is expected as a notification"
        )

        self.set_initialized(True)

    async def _handle_initialize(self, request: MethodRequest) -> MethodResponse:
        """Handle the initialize method."""
        # Return protocol version
        result = InitializeResult(server_protocol_version=1)

        return MethodSuccess(jsonrpc="2.0", id=request.id, result=result)

    async def _handle_component_list(self, request: MethodRequest) -> MethodResponse:
        """Handle the components/list method."""
        # Parse parameters - handle empty params case
        assert isinstance(request.params, ComponentListParams)

        # Build component list
        component_infos = []
        for name, component in self._components.items():
            component_infos.append(
                ComponentInfo(
                    component=name,
                    input_schema=component.input_schema(),
                    output_schema=component.output_schema(),
                    description=component.description,
                )
            )

        result = ListComponentsResult(components=component_infos)
        return MethodSuccess(jsonrpc="2.0", id=request.id, result=result)

    async def _handle_component_info(self, request: MethodRequest) -> MethodResponse:
        """Handle the components/info method."""
        assert isinstance(request.params, ComponentInfoParams)
        params: ComponentInfoParams = request.params

        component = self._components.get(params.component)
        if component is None:
            raise ComponentNotFoundError(f"Component '{params.component}' not found")

        info = ComponentInfo(
            component=params.component,
            input_schema=component.input_schema(),
            output_schema=component.output_schema(),
            description=component.description,
        )

        result = ComponentInfoResult(info=info)
        return MethodSuccess(jsonrpc="2.0", id=request.id, result=result)

    async def _handle_component_execute(
        self, request: MethodRequest, context: StepflowContext | None = None
    ) -> MethodResponse:
        """Handle the components/execute method."""
        assert isinstance(request.params, ComponentExecuteParams)
        params: ComponentExecuteParams = request.params

        component = self._components.get(params.component)
        if component is None:
            raise ComponentNotFoundError(f"Component '{params.component}' not found")

        try:
            # Parse input using component's input type
            input_value: Any = msgspec.convert(params.input, type=component.input_type)

            # Execute component with or without context
            args = [input_value]
            if context is not None:
                args.append(context)

            if inspect.iscoroutinefunction(component.function):
                output = await component.function(*args)
            else:
                output = component.function(*args)

            result = ComponentExecuteResult(output=output)
            print(
                f"Executed component {params.component} "
                f"with input {input_value} produced {output}",
                file=sys.stderr,
            )
            return MethodSuccess(jsonrpc="2.0", id=request.id, result=result)

        except SkipStep as e:
            # Component requested to be skipped - return FlowResultSkipped
            skip_result = FlowResultSkipped(reason=e.message)
            result = ComponentExecuteResult(output=skip_result)
            print(
                f"Skipped component {params.component}: {e.message}",
                file=sys.stderr,
            )
            return MethodSuccess(jsonrpc="2.0", id=request.id, result=result)
        except Exception as e:
            print("Error executing component:", file=sys.stderr)
            traceback.print_exc(file=sys.stderr)
            raise StepflowExecutionError(f"Component execution failed: {str(e)}") from e

    def langchain_component(
        self,
        func: Callable | None = None,
        *,
        name: str | None = None,
        description: str | None = None,
        execution_mode: str = "invoke",
    ):
        """
        Decorator to register a LangChain runnable factory as a Stepflow component.

        The decorated function should return a LangChain Runnable instance.
        The resulting component will execute the runnable with the provided input.

        Args:
            func: The function to register (provided by the decorator)
            name: Optional name for the component. If not provided, uses the
                function name
            description: Optional description. If not provided, uses the
                function's docstring
            execution_mode: Default execution mode ("invoke", "batch", "stream")

        Example:
            @server.langchain_component(name="my_chain")
            def create_my_chain() -> Runnable:
                return prompt | llm | parser

        Raises:
            StepflowExecutionError: If LangChain is not available
        """
        if not _HAS_LANGCHAIN:
            raise StepflowExecutionError(
                "LangChain integration requires langchain-core. "
                "Install with: pip install stepflow-py[langchain]"
            )

        def decorator(f: Callable) -> Callable:
            from stepflow_py.langchain_integration import (
                check_langchain_available,
                create_runnable_config,
                get_runnable_schemas,
            )

            check_langchain_available()

            component_name = name or f.__name__
            if not component_name.startswith("/"):
                component_name = f"/{component_name}"

            # Validate execution mode
            if execution_mode not in ["invoke", "batch", "stream"]:
                raise ValueError(
                    f"Invalid execution_mode '{execution_mode}'. "
                    "Must be 'invoke', 'batch', or 'stream'"
                )

            # Create the runnable instance by calling the factory function
            try:
                runnable = f()
            except Exception as e:
                raise StepflowExecutionError(
                    f"Failed to create LangChain runnable: {e}\n"
                    f"Traceback:\n{traceback.format_exc()}"
                ) from e

            # Validate that it's actually a runnable
            try:
                from langchain_core.runnables import Runnable

                if not isinstance(runnable, Runnable):
                    raise StepflowExecutionError(
                        f"Function {f.__name__} must return a LangChain Runnable "
                        f"instance, got {type(runnable)}"
                    )
            except ImportError:
                raise StepflowExecutionError("LangChain not available") from None

            # Get schemas from the runnable
            try:
                input_schema, output_schema = get_runnable_schemas(runnable)
            except Exception:
                # Fallback to generic schemas
                input_schema = {"type": "object", "additionalProperties": True}
                output_schema = {"type": "object", "additionalProperties": True}

            # Define LangChainComponentInput class inline

            class LangChainComponentInput(msgspec.Struct):
                """Input structure for LangChain components."""

                input: Any
                config: dict[str, Any] | None = None
                execution_mode: str = "invoke"

            # Create the component function that will execute the runnable
            async def langchain_component_executor(
                component_input: LangChainComponentInput, context: StepflowContext
            ) -> Any:
                """Execute the registered LangChain runnable."""

                # Validate execution mode (only invoke is supported)
                exec_mode = component_input.execution_mode or execution_mode
                if exec_mode != "invoke":
                    raise StepflowExecutionError(
                        f"Invalid execution_mode '{exec_mode}'. "
                        "Only 'invoke' mode is supported."
                    )

                # Prepare input for LangChain runnable
                langchain_input = component_input.input

                # Create runnable config from Stepflow input
                stepflow_input_dict = {
                    "input": component_input.input,
                    "config": component_input.config or {},
                }
                runnable_config = create_runnable_config(stepflow_input_dict, context)

                # Execute the runnable
                return await runnable.ainvoke(langchain_input, config=runnable_config)

            # Set up the component input/output types and description
            langchain_component_executor.__annotations__ = {
                "component_input": LangChainComponentInput,
                "context": StepflowContext,
                "return": Any,
            }

            # Use provided description or function docstring
            component_description = description or (
                f.__doc__.strip() if f.__doc__ else None
            )
            if component_description:
                langchain_component_executor.__doc__ = component_description

            # Register the component using the server's component method
            self.component(
                langchain_component_executor,
                name=component_name,
                description=component_description,
            )

            # Store additional metadata about the LangChain component
            if not hasattr(self, "_langchain_components"):
                self._langchain_components: dict[str, dict[str, Any]] = {}

            self._langchain_components[component_name] = {
                "factory_function": f,
                "runnable": runnable,
                "input_schema": input_schema,
                "output_schema": output_schema,
                "execution_mode": execution_mode,
                "description": component_description,
            }

            @wraps(f)
            def wrapper(*args, **kwargs):
                return f(*args, **kwargs)

            return wrapper

        if func is None:
            return decorator
        return decorator(func)

    def start_stdio(
        self,
        stdin: Any = None,  # asyncio.StreamReader | None
        stdout: Any = None,  # asyncio.StreamWriter | None
    ) -> None:
        """Start the server using STDIO transport.

        Args:
            stdin: Optional StreamReader to read from (defaults to sys.stdin)
            stdout: Optional StreamWriter to write to (defaults to sys.stdout)
        """
        from .stdio_server import StepflowStdioServer

        stdio_server = StepflowStdioServer(server=self)
        stdio_server.run(stdin=stdin, stdout=stdout)

    async def start_http(self, host: str = "localhost", port: int = 8080) -> None:
        """Start the server using HTTP transport.

        Args:
            host: Server host (default: localhost)
            port: Server port (default: 8080)
        """
        from .http_server import StepflowHttpServer

        http_server = StepflowHttpServer(server=self, host=host, port=port)
        await http_server.run()
