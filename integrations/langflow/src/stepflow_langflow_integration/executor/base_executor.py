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

from typing import Any

from .type_converter import TypeConverter


class BaseExecutor:
    """Base class for Langflow component executors.

    Provides shared functionality for both core and custom code executors:
    - Execution method determination
    - Component input defaults application
    - Type conversion utilities
    """

    def __init__(self):
        """Initialize base executor with shared components."""
        self.type_converter = TypeConverter()

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

    def _setup_graph_context(
        self, component_instance: Any, session_id: str
    ) -> None:
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
