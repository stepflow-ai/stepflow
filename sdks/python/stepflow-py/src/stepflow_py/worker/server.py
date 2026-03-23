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
import logging
import traceback
from collections.abc import Callable
from dataclasses import dataclass
from functools import wraps
from typing import Any

import msgspec

from stepflow_py.worker.context import StepflowContext
from stepflow_py.worker.exceptions import (
    StepflowExecutionError,
)
from stepflow_py.worker.path_trie import PathTrie
from stepflow_py.worker.udf import udf

# Check if LangChain is available
try:
    from stepflow_py.worker.langchain_components import register_langchain_components

    _HAS_LANGCHAIN = True
except ImportError:
    _HAS_LANGCHAIN = False

logger = logging.getLogger(__name__)


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


class StepflowServer:
    """Stepflow server with component registry.

    Components are registered via the @server.component decorator and
    executed by transport workers (gRPC, NATS) via task_handler.
    """

    def __init__(self, include_builtins: bool = True):
        self._components: dict[str, ComponentEntry] = {}
        self._wildcard_trie: PathTrie[ComponentEntry] = PathTrie()

        # Add LangChain registry functionality if available
        if _HAS_LANGCHAIN:
            register_langchain_components(self)

        if include_builtins:
            # Register the UDF component
            self.component(udf)

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

            # First parameter is always the input type
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

            if inspect.iscoroutinefunction(f):
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
