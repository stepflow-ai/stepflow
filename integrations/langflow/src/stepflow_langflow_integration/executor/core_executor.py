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

"""Core component executor for known Langflow components.

Executes known Langflow components by importing them directly by module path,
without needing to compile code from blob storage. This is more efficient for
components whose code hash matches a known version.
"""

import importlib
import inspect
from typing import Any

from stepflow_py.worker import StepflowContext
from stepflow_py.worker.observability import get_tracer

from ..exceptions import ExecutionError
from .base_executor import BaseExecutor


class CoreExecutor(BaseExecutor):
    """Executes known Langflow components by importing them directly."""

    def __init__(self):
        """Initialize core executor."""
        super().__init__()

    async def execute(
        self,
        component_path: str,
        input_data: dict[str, Any],
        context: StepflowContext,
    ) -> dict[str, Any]:
        """Execute a core Langflow component.

        Args:
            component_path: The component path suffix after /langflow/core/
                           (e.g., "lfx/components/docling/DoclingInlineComponent")
            input_data: Component input containing template, outputs, runtime inputs
            context: Stepflow context (may be needed for some operations)

        Returns:
            Component execution result
        """
        tracer = get_tracer(__name__)

        # Convert path to module path (slashes to dots)
        # e.g., "lfx/components/docling/DoclingInlineComponent"
        #    -> "lfx.components.docling.DoclingInlineComponent"
        module_path = component_path.replace("/", ".")

        # Split into module and class name
        parts = module_path.rsplit(".", 1)
        if len(parts) != 2:
            raise ExecutionError(f"Invalid component path: {component_path}")

        module_name, class_name = parts

        with tracer.start_as_current_span(
            f"core_execute:{class_name}",
            attributes={
                "component_path": component_path,
                "module_name": module_name,
                "class_name": class_name,
            },
        ):
            # Import the component class
            try:
                module = importlib.import_module(module_name)
                component_class = getattr(module, class_name)
            except ImportError as e:
                raise ExecutionError(
                    f"Failed to import module {module_name}: {e}"
                ) from e
            except AttributeError as e:
                raise ExecutionError(
                    f"Class {class_name} not found in module {module_name}: {e}"
                ) from e

            # Extract execution parameters from input
            template = input_data.get("template", {})
            outputs = input_data.get("outputs", [])
            selected_output = input_data.get("selected_output")
            runtime_inputs = input_data.get("input", {})

            # Determine execution method
            execution_method = self._determine_execution_method(
                outputs, selected_output
            )
            if not execution_method:
                raise ExecutionError(f"No execution method found for {class_name}")

            # Execute the component
            result = await self._execute_component(
                component_class=component_class,
                template=template,
                execution_method=execution_method,
                runtime_inputs=runtime_inputs,
                class_name=class_name,
            )

            return {"result": self.type_converter.serialize_langflow_object(result)}

    async def _execute_component(
        self,
        component_class: type,
        template: dict[str, Any],
        execution_method: str,
        runtime_inputs: dict[str, Any],
        class_name: str,
    ) -> Any:
        """Execute a component instance."""
        tracer = get_tracer(__name__)

        # Create component instance
        with tracer.start_as_current_span(
            f"instantiate_component:{class_name}",
            attributes={"class_name": class_name},
        ):
            try:
                component_instance = component_class()
            except Exception as e:
                raise ExecutionError(f"Failed to instantiate {class_name}: {e}") from e

        # Prepare parameters
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

        # Configure component
        if hasattr(component_instance, "set_attributes"):
            component_instance._parameters = component_parameters
            component_instance.set_attributes(component_parameters)

        # Execute component method
        if not hasattr(component_instance, execution_method):
            available = [m for m in dir(component_instance) if not m.startswith("_")]
            raise ExecutionError(
                f"Method {execution_method} not found in {class_name}. "
                f"Available: {available}"
            )

        method = getattr(component_instance, execution_method)

        with tracer.start_as_current_span(
            f"execute_method:{execution_method}",
            attributes={
                "class_name": class_name,
                "execution_method": execution_method,
            },
        ):
            try:
                if inspect.iscoroutinefunction(method):
                    result = await method()
                else:
                    result = method()
                return result
            except Exception as e:
                raise ExecutionError(
                    f"Error executing {execution_method} on {class_name}: {e}"
                ) from e

    async def _prepare_component_parameters(
        self, template: dict[str, Any], runtime_inputs: dict[str, Any]
    ) -> dict[str, Any]:
        """Prepare component parameters from template and runtime inputs."""
        parameters: dict[str, Any] = {}

        # Extract default values from template
        for key, field in template.items():
            if isinstance(field, dict) and "value" in field:
                parameters[key] = field["value"]
            elif isinstance(field, dict):
                # Field without value - skip or use None
                pass
            else:
                # Direct value
                parameters[key] = field

        # Override with runtime inputs
        parameters.update(runtime_inputs)

        return parameters
