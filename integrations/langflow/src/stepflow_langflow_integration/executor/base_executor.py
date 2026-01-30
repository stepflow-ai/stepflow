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

"""Base executor class with shared functionality for Langflow component execution."""

import inspect
import os
from abc import ABC, abstractmethod
from typing import Any

from stepflow_py.worker.observability import get_tracer

from ..exceptions import ExecutionError
from .type_converter import TypeConverter


class BaseExecutor(ABC):
    """Abstract base class for Langflow component executors.

    Provides shared functionality for both core and custom code executors:
    - Execution method determination
    - Component input defaults application
    - Type conversion utilities
    - Common component execution flow

    Subclasses must implement `_instantiate_component` to define how components
    are loaded and instantiated.
    """

    def __init__(self):
        """Initialize base executor with shared components."""
        self.type_converter = TypeConverter()

    @abstractmethod
    async def _instantiate_component(
        self,
        component_info: dict[str, Any],
    ) -> tuple[Any, str]:
        """Instantiate a component from component info.

        This is the only method that differs between executors:
        - CoreExecutor: imports module and instantiates class
        - CustomCodeExecutor: compiles code from blob and instantiates class

        Args:
            component_info: Information needed to instantiate the component.
                For CoreExecutor: contains module_name, class_name
                For CustomCodeExecutor: contains component class and metadata

        Returns:
            Tuple of (component_instance, component_name) for execution
        """
        pass

    async def _execute_component_instance(
        self,
        component_instance: Any,
        component_name: str,
        execution_method: str,
        template: dict[str, Any],
        runtime_inputs: dict[str, Any],
    ) -> Any:
        """Execute a component instance with the given parameters.

        This is the shared execution flow used by both executors after
        the component has been instantiated.

        Args:
            component_instance: Instantiated component object
            component_name: Name for logging/tracing
            execution_method: Method name to call on the component
            template: Template containing field definitions
            runtime_inputs: Runtime input values

        Returns:
            Component execution result (before serialization)
        """
        tracer = get_tracer(__name__)

        # Prepare parameters from template and runtime inputs
        component_parameters = await self._prepare_component_parameters(
            template, runtime_inputs
        )

        # Apply component input defaults
        component_parameters = self._apply_component_input_defaults(
            component_instance, component_parameters
        )

        # Set session_id and graph context
        session_id = component_parameters.get("session_id", "default_session")
        component_instance._session_id = session_id
        self._setup_graph_context(component_instance, session_id)

        # Configure component with parameters
        if hasattr(component_instance, "set_attributes"):
            component_instance._parameters = component_parameters
            component_instance.set_attributes(component_parameters)

        # Verify execution method exists
        if not hasattr(component_instance, execution_method):
            available = [m for m in dir(component_instance) if not m.startswith("_")]
            raise ExecutionError(
                f"Method {execution_method} not found in {component_name}. "
                f"Available: {available}"
            )

        method = getattr(component_instance, execution_method)

        # Execute the method with tracing
        with tracer.start_as_current_span(
            f"execute_method:{execution_method}",
            attributes={
                "component_name": component_name,
                "execution_method": execution_method,
            },
        ):
            try:
                if inspect.iscoroutinefunction(method):
                    return await method()
                else:
                    return method()
            except Exception as e:
                raise ExecutionError(
                    f"Error executing {execution_method} on {component_name} "
                    f"({type(component_instance).__name__}): {e}"
                ) from e

    async def _prepare_component_parameters(
        self, template: dict[str, Any], runtime_inputs: dict[str, Any]
    ) -> dict[str, Any]:
        """Prepare component parameters from template and runtime inputs.

        This base implementation handles common parameter extraction.
        Subclasses may override for more complex type conversion.

        Args:
            template: Template containing field definitions
            runtime_inputs: Runtime input values

        Returns:
            Merged parameters dictionary
        """
        parameters: dict[str, Any] = {}

        # Extract default values from template
        for key, field in template.items():
            if isinstance(field, dict) and "value" in field:
                # Skip handle inputs with empty values (connected steps provide them)
                input_types = field.get("input_types", [])
                value = field["value"]

                if input_types and (value == "" or value is None):
                    continue

                parameters[key] = value
            elif isinstance(field, dict):
                # Field without value - skip
                pass
            else:
                # Direct value
                parameters[key] = field

        # Override with runtime inputs
        parameters.update(runtime_inputs)

        # Resolve environment variables for load_from_db fields
        parameters = self._resolve_env_variables(parameters, template)

        return parameters

    def _determine_execution_method(
        self, outputs: list[dict[str, Any]], selected_output: str | None
    ) -> str | None:
        """Determine which method to call based on outputs configuration.

        Args:
            outputs: List of output definitions from the component
            selected_output: The name of the selected output (if any)

        Returns:
            The method name to call, or None if not found
        """
        if not outputs:
            return None

        # If selected_output is provided, find the matching output method
        if selected_output:
            for output in outputs:
                if output.get("name") == selected_output:
                    method = output.get("method")
                    if isinstance(method, str) and method:
                        return method

        # Default to first output's method
        method = outputs[0].get("method")
        if isinstance(method, str) and method:
            return method

        return None

    def _apply_component_input_defaults(
        self, component_instance: Any, parameters: dict[str, Any]
    ) -> dict[str, Any]:
        """Apply default values from component's input definitions.

        Args:
            component_instance: Instantiated component with inputs attribute
            parameters: Current component parameters

        Returns:
            Parameters with defaults applied for missing values
        """
        if not hasattr(component_instance, "inputs"):
            return parameters

        inputs = component_instance.inputs
        if not inputs:
            return parameters

        result = dict(parameters)

        for input_def in inputs:
            field_name = getattr(input_def, "name", None)
            if not field_name:
                continue

            # Only set default if not already in parameters
            if field_name not in result:
                default_value = getattr(input_def, "value", None)
                if default_value is not None and default_value != "":
                    result[field_name] = default_value

        return result

    def _setup_graph_context(self, component_instance: Any, session_id: str) -> None:
        """Set up graph context for components that need it.

        Some components (like Agent) access self.graph.session_id and
        self.graph.vertices. This creates a minimal graph context object.

        Args:
            component_instance: Component instance to configure
            session_id: Session ID for the graph context
        """

        class GraphContext:
            def __init__(self, session_id: str):
                self.session_id = session_id
                self.vertices: list[Any] = []
                self.flow_id = None

        # Use __dict__ to set graph even if it's a read-only property
        component_instance.__dict__["graph"] = GraphContext(session_id)

    def _resolve_env_variables(
        self, parameters: dict[str, Any], template: dict[str, Any]
    ) -> dict[str, Any]:
        """Resolve environment variables for parameters marked with load_from_db.

        For fields that have load_from_db=True and an empty/falsy value, this method
        attempts to load the value from environment variables. The variable name is
        taken from the template field's "value" key.

        This enables API keys and other secrets to be loaded from the environment
        rather than being passed explicitly as runtime variables.

        Args:
            parameters: Current component parameters
            template: Template containing field definitions with load_from_db flags

        Returns:
            Parameters with environment variables resolved

        Raises:
            ExecutionError: If required environment variable is not set
        """
        result = dict(parameters)

        for key, value in result.items():
            # Skip if value is already set (truthy)
            if value:
                continue

            # Check if template field is marked for loading from DB/env
            template_field = template.get(key, {})
            if not isinstance(template_field, dict):
                continue

            if not template_field.get("load_from_db", False):
                continue

            # Get the variable name from the template field's value
            # This is the name of the environment variable to load
            var_name = template_field.get("value", key)
            if not var_name:
                continue

            # Attempt to load from environment
            env_value = os.environ.get(var_name)
            if env_value is None:
                raise ExecutionError(
                    f"Environment variable '{var_name}' for parameter '{key}' not set. "
                    f"Either provide the value as a runtime variable or set the "
                    f"environment variable."
                )
            result[key] = env_value

        return result
