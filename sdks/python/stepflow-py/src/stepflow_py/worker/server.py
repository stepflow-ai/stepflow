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
import traceback
from collections.abc import Callable
from dataclasses import dataclass
from functools import wraps
from typing import Any, assert_never

import msgspec

from stepflow_py.worker.context import StepflowContext
from stepflow_py.worker.exceptions import (
    ComponentNotFoundError,
    StepflowError,
    StepflowExecutionError,
    StepflowProtocolError,
)
from stepflow_py.worker.generated_protocol import (
    ComponentExecuteParams,
    ComponentExecuteResult,
    ComponentInferSchemaParams,
    ComponentInferSchemaResult,
    ComponentInfo,
    ComponentInfoParams,
    ComponentInfoResult,
    ComponentListParams,
    Error,
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
from stepflow_py.worker.path_trie import PathTrie
from stepflow_py.worker.udf import udf

# Check if LangChain is available
try:
    from stepflow_py.worker.langchain_components import register_langchain_components

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


def _has_path_params(name: str) -> bool:
    """Check if a component name contains path parameters."""
    return "{" in name and "}" in name


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
        self._wildcard_trie: PathTrie[ComponentEntry] = PathTrie()
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

        Supports path parameters in component names:
        - `{name}` - captures a single path segment
        - `{*name}` - captures remaining path (wildcard/catch-all)

        Path parameters are passed as keyword arguments to the function.

        Example:
            @server.component(name="core/{*component}")
            async def handler(input_data: dict, context: StepflowContext,
                              component: str) -> dict:
                ...

        Args:
            func: The function to register (provided by the decorator)
            name: Optional name for the component. If not provided, uses the function
                name. Supports path parameters like {name} and {*name}.
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

            entry = ComponentEntry(
                name=component_name,
                function=f,
                input_type=input_type,
                output_type=return_type,
                description=component_description,
            )

            # Store in appropriate registry based on whether name has path params
            if _has_path_params(component_name):
                self._wildcard_trie.insert(component_name, entry)
            else:
                self._components[component_name] = entry

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

    def get_component(
        self, component_path: str
    ) -> tuple[ComponentEntry, dict[str, str]] | None:
        """Get a registered component by path.

        Supports both exact matches and pattern matching for components
        registered with path parameters.

        Args:
            component_path: The component path to look up

        Returns:
            Tuple of (ComponentEntry, path_params) if found, None otherwise.
            path_params is an empty dict for exact matches.
        """
        # Try exact match first (faster)
        if component_path in self._components:
            return self._components[component_path], {}

        # Try pattern matching using trie (O(n) where n = path segments)
        result = self._wildcard_trie.match(component_path)
        if result is not None:
            entry, params = result
            return entry, params

        return None

    def get_components(self) -> dict[str, ComponentEntry]:
        """Get all registered components (both exact and pattern-based)."""
        return {**self._components, **self._wildcard_trie.get_patterns()}

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
                    result = self.get_component(message.params.component)
                    if result is None:
                        # Component not found - doesn't require context (errors later)
                        return False

                    component, _ = result
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
            elif request.method == Method.components_infer_schema:
                return await self._handle_component_infer_schema(request)
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

        return MethodSuccess(id=request.id, result=result)

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
        return MethodSuccess(id=request.id, result=result)

    async def _handle_component_info(self, request: MethodRequest) -> MethodResponse:
        """Handle the components/info method."""
        assert isinstance(request.params, ComponentInfoParams)
        params: ComponentInfoParams = request.params

        result = self.get_component(params.component)
        if result is None:
            raise ComponentNotFoundError(f"Component '{params.component}' not found")

        component, _ = result
        info = ComponentInfo(
            component=params.component,
            input_schema=component.input_schema(),
            output_schema=component.output_schema(),
            description=component.description,
        )

        result_info = ComponentInfoResult(info=info)
        return MethodSuccess(id=request.id, result=result_info)

    async def _handle_component_infer_schema(
        self, request: MethodRequest
    ) -> MethodResponse:
        """Handle the components/infer_schema method.

        Infers the output schema for a component given an input schema.
        For now, this returns the static output schema from the component's
        type annotations. In the future, this could be extended to support
        dynamic schema inference based on the input schema.
        """
        assert isinstance(request.params, ComponentInferSchemaParams)
        params: ComponentInferSchemaParams = request.params

        result = self.get_component(params.component)
        if result is None:
            raise ComponentNotFoundError(f"Component '{params.component}' not found")

        component, _ = result
        # Return the static output schema from the component's type annotations
        # In the future, components could implement dynamic schema inference
        schema_result = ComponentInferSchemaResult(
            output_schema=component.output_schema()
        )
        return MethodSuccess(id=request.id, result=schema_result)

    async def _handle_component_execute(
        self, request: MethodRequest, context: StepflowContext | None = None
    ) -> MethodResponse:
        """Handle the components/execute method."""
        assert isinstance(request.params, ComponentExecuteParams)
        params: ComponentExecuteParams = request.params

        lookup_result = self.get_component(params.component)
        if lookup_result is None:
            raise ComponentNotFoundError(f"Component '{params.component}' not found")

        component, path_params = lookup_result

        try:
            import logging

            from opentelemetry import trace as otel_trace

            from stepflow_py.worker.observability import (
                extract_trace_context,
                get_tracer,
                set_diagnostic_context,
            )

            # Set diagnostic context for logging
            set_diagnostic_context(
                flow_id=params.observability.flow_id,
                run_id=params.observability.run_id,
                step_id=params.observability.step_id,
            )

            logger = logging.getLogger(__name__)

            # Create a child span if trace context is available
            tracer = get_tracer(__name__)
            span_context = None
            if params.observability.trace_id and params.observability.span_id:
                span_context = extract_trace_context(
                    params.observability.trace_id, params.observability.span_id
                )

            # Create OpenTelemetry context from span context
            otel_context = None
            if span_context:
                otel_context = otel_trace.set_span_in_context(
                    otel_trace.NonRecordingSpan(span_context)
                )

            # Create span for component execution
            with tracer.start_as_current_span(
                f"component:{params.component}",
                context=otel_context,
                attributes={
                    "component": params.component,
                    "attempt": params.attempt,
                    **(
                        {"run_id": params.observability.run_id}
                        if params.observability.run_id
                        else {}
                    ),
                    **(
                        {"flow_id": params.observability.flow_id}
                        if params.observability.flow_id
                        else {}
                    ),
                    **(
                        {"step_id": params.observability.step_id}
                        if params.observability.step_id
                        else {}
                    ),
                },
            ):
                # Parse input using component's input type
                input_value: Any = msgspec.convert(
                    params.input, type=component.input_type
                )

                logger.info(
                    f"Executing component {params.component} (attempt {params.attempt})"
                )
                logger.debug(f"Component input: {input_value}")

                # Execute component with or without context, plus path params as kwargs
                args = [input_value]
                if context is not None:
                    args.append(context)

                if inspect.iscoroutinefunction(component.function):
                    output = await component.function(*args, **path_params)
                else:
                    output = component.function(*args, **path_params)

                result = ComponentExecuteResult(output=output)
                logger.info(f"Component {params.component} executed successfully")
                logger.debug(f"Component output: {output}")
                return MethodSuccess(id=request.id, result=result)

        except Exception as e:
            logger = logging.getLogger(__name__)
            logger.error(
                f"Error executing component {params.component}: {e}", exc_info=True
            )
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
            from stepflow_py.worker.langchain_integration import (
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

    async def run(
        self,
        host: str = "127.0.0.1",
        port: int = 0,
        workers: int = 3,
        backlog: int = 128,
        timeout_keep_alive: int = 5,
        instance_id: str | None = None,
    ) -> None:
        """Start the server using HTTP transport.

        When port is 0, the server binds to an available port and announces
        the actual port via stdout in JSON format: {"port": N}

        Args:
            host: Server host (default: 127.0.0.1)
            port: Server port (0 for auto-assign, default: 0)
            workers: Number of worker processes (default: 3)
            backlog: Maximum number of pending connections (default: 128)
            timeout_keep_alive: Keep-alive timeout in seconds (default: 5)
            instance_id: Optional instance ID for load balancer routing
                (auto-generated if not provided)
        """
        from .http_server import run_http_server

        await run_http_server(
            server=self,
            host=host,
            port=port,
            workers=workers,
            backlog=backlog,
            timeout_keep_alive=timeout_keep_alive,
            instance_id=instance_id,
        )
