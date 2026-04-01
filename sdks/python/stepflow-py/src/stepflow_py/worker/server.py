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
    path: str | None = None

    def input_schema(self):
        return msgspec.json.schema(self.input_type)

    def output_schema(self):
        return msgspec.json.schema(self.output_type)


class StepflowServer:
    """Stepflow server with component registry.

    Components are registered via the @server.component decorator and
    executed by transport workers (gRPC, NATS) via task_handler.
    """

    def __init__(self, include_builtins: bool = True):
        self._components: dict[str, ComponentEntry] = {}

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
        subpath: str | None = None,
        id: str | None = None,
        description: str | None = None,
        # Deprecated alias for subpath — use subpath instead.
        name: str | None = None,
    ):
        """Decorator to register a component function.

        Two concepts are at play:

        * **Component ID** — the unique identifier used as the dispatch key.
          The orchestrator sends this value as ``component_id`` in
          ``ComponentExecuteRequest``.  Defaults to the function name.
        * **Subpath** — the path pattern the component is mounted at, relative
          to the worker's route prefix.  Defaults to ``"/{id}"``.

        These are distinct: the ID is for dispatch, the subpath is for routing.

        For example, if the orchestrator config maps prefix ``/python`` to this
        worker, and a component is registered as ``my_func``, then workflows
        reference it as ``/python/my_func`` — **not** ``/my_func``::

            Orchestrator config       Component subpath       Workflow path
            ──────────────────        ─────────────────       ─────────────
            routes:                   @server.component       /python/my_func
              "/python":              def my_func(...):
                - plugin: python          ...

        Do **not** include the prefix in component subpaths — the orchestrator
        adds it automatically.

        Supports path parameters in component subpaths:
        - ``{name}`` - captures a single path segment
        - ``{*name}`` - captures remaining path (wildcard/catch-all)

        Path parameters are extracted by the orchestrator and passed as keyword
        arguments to the function.

        Example:
            @server.component(subpath="core/{*component}")
            async def handler(input_data: dict, context: StepflowContext,
                              component: str) -> dict:
                # Subpath: /core/{*component}
                # With prefix "/python", full workflow path is /python/core/foo/bar
                # component="foo/bar" is extracted by the orchestrator
                ...

        Args:
            func: The function to register (provided by the decorator)
            subpath: Optional subpath where the component is mounted under the
                worker's route prefix.  Defaults to ``"/{id}"``.
                Supports path parameters like {name} and {*name}.
            id: Optional component ID (dispatch key).  Defaults to the function
                name.
            description: Optional description. If not provided, uses the function's
                docstring
            name: Deprecated alias for ``subpath``.  Use ``subpath`` instead.
        """

        def decorator(f: Callable) -> Callable:
            # Resolve subpath: explicit subpath > deprecated name > default
            resolved_subpath = subpath or name

            # Resolve component ID
            component_id = id or f.__name__

            if resolved_subpath is not None:
                component_path = resolved_subpath
                if not component_path.startswith("/"):
                    component_path = f"/{component_path}"
            else:
                component_path = f"/{component_id}"

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
                name=component_id,
                function=f,
                input_type=input_type,
                output_type=return_type,
                description=component_description,
                path=component_path,
            )

            self._components[component_id] = entry

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

    def get_component(self, component_id: str) -> ComponentEntry | None:
        """Get a registered component by component ID.

        Simple dict lookup — path matching is handled by the orchestrator.

        Args:
            component_id: The component ID to look up

        Returns:
            ComponentEntry if found, None otherwise.
        """
        return self._components.get(component_id)

    def get_components(self) -> dict[str, ComponentEntry]:
        """Get all registered components."""
        return dict(self._components)

    def langchain_component(
        self,
        func: Callable | None = None,
        *,
        subpath: str | None = None,
        description: str | None = None,
        execution_mode: str = "invoke",
        # Deprecated alias for subpath — use subpath instead.
        name: str | None = None,
    ):
        """
        Decorator to register a LangChain runnable factory as a Stepflow component.

        The decorated function should return a LangChain Runnable instance.
        The resulting component will execute the runnable with the provided input.

        Args:
            func: The function to register (provided by the decorator)
            subpath: Optional subpath for the component. If not provided, uses
                the function name
            description: Optional description. If not provided, uses the
                function's docstring
            execution_mode: Default execution mode ("invoke", "batch", "stream")
            name: Deprecated alias for ``subpath``.  Use ``subpath`` instead.

        Example:
            @server.langchain_component(subpath="my_chain")
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

            component_name = subpath or name or f.__name__
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

            # Register the component using the server's component method.
            # Use the factory function's name as the component ID to ensure
            # uniqueness (the inner executor closure has the same name for all
            # langchain components).
            self.component(
                langchain_component_executor,
                subpath=component_name,
                id=f.__name__,
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
