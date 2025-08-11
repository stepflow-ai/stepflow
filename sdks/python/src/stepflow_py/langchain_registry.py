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

"""
LangChain runnable registry system for StepFlow.

This module provides the @server.langchain_component decorator that allows
users to register LangChain runnables as StepFlow components.
"""

from __future__ import annotations

from collections.abc import Callable
from functools import wraps
from typing import Any

import msgspec

from stepflow_py.context import StepflowContext
from stepflow_py.exceptions import StepflowExecutionError
from stepflow_py.langchain_integration import (
    check_langchain_available,
    create_runnable_config,
    get_runnable_schemas,
)


class LangChainComponentInput(msgspec.Struct):
    """
    Input structure for LangChain components registered via @langchain_component.

    This provides a flexible input structure that can accommodate various
    LangChain runnable input types while allowing optional configuration.
    """

    input: Any
    config: dict[str, Any] | None = None
    execution_mode: str = "invoke"


def add_langchain_component_method(server_class):
    """
    Add the langchain_component decorator method to a StepFlow server class.

    This function extends the server class with LangChain-specific functionality
    without modifying the core server implementation.

    Args:
        server_class: The StepFlow server class to extend
    """

    def langchain_component(
        self,
        func: Callable | None = None,
        *,
        name: str | None = None,
        description: str | None = None,
        execution_mode: str = "invoke",
    ):
        """
        Decorator to register a LangChain runnable factory as a StepFlow component.

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
        """

        def decorator(f: Callable) -> Callable:
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
                    f"Failed to create LangChain runnable: {e}"
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

                # Create runnable config from StepFlow input
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
                self._langchain_components = {}

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

    # Add the method to the server class
    server_class.langchain_component = langchain_component


def get_langchain_components(server) -> dict[str, dict[str, Any]]:
    """
    Get information about all registered LangChain components.

    Args:
        server: The StepFlow server instance

    Returns:
        Dictionary mapping component names to their metadata
    """
    if not hasattr(server, "_langchain_components"):
        return {}

    return server._langchain_components.copy()  # type: ignore[no-any-return]


def list_langchain_components(server) -> list[str]:
    """
    Get a list of all registered LangChain component names.

    Args:
        server: The StepFlow server instance

    Returns:
        List of component names
    """
    if not hasattr(server, "_langchain_components"):
        return []

    return list(server._langchain_components.keys())
