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

"""New UDF executor with pre-compilation approach (no CachedStepflowContext needed)."""

import inspect
import sys
from typing import Any

from stepflow_py import StepflowContext

from ..exceptions import ExecutionError
from .type_converter import TypeConverter


class UDFExecutor:
    """Executes Langflow components using pre-compilation to eliminate context calls."""

    def __init__(self):
        """Initialize UDF executor."""
        self.type_converter = TypeConverter()
        self.compiled_components: dict[str, Any] = {}

    async def execute(
        self, input_data: dict[str, Any], context: StepflowContext
    ) -> dict[str, Any]:
        """Execute using the new pre-compilation approach.

        This method first pre-compiles all components, then executes without context.

        Args:
            input_data: Component input containing blob_id and runtime inputs
            context: Stepflow context for pre-compilation only

        Returns:
            Component execution result
        """

        # Step 1: Pre-compile all components from blobs
        await self._prepare_components(input_data, context)

        # Step 2: Execute using pre-compiled components (no context needed)
        return await self._execute_with_precompiled_components(input_data)

    async def _prepare_components(
        self, input_data: dict[str, Any], context: StepflowContext
    ) -> None:
        """Pre-compile all components from blob data to eliminate runtime context calls.

        This method fetches all blob data containing component source code and
        compiles them into executable component instances.

        Args:
            input_data: Component input containing blob IDs
            context: Stepflow context for blob operations
                (used only for pre-compilation)
        """
        blob_ids_to_compile = self._extract_blob_ids(input_data)

        for blob_id in blob_ids_to_compile:
            if blob_id not in self.compiled_components:
                try:
                    blob_data = await context.get_blob(blob_id)
                except Exception as e:
                    # No fallback component generation - require real component code
                    # Failed to load blob - component code required
                    raise ExecutionError(
                        f"No component code found for blob {blob_id}. "
                        f"All components must have real Langflow code."
                    ) from e

                compiled_component = await self._compile_component(blob_data)
                self.compiled_components[blob_id] = compiled_component

    def _extract_blob_ids(self, input_data: dict[str, Any]) -> set[str]:
        """Extract all blob IDs that need to be compiled from input data."""
        blob_ids = set()

        # Main blob_id
        if "blob_id" in input_data:
            blob_ids.add(input_data["blob_id"])

        return blob_ids

    def _serialize_langflow_objects(self, obj: Any) -> Any:
        """Convert Langflow objects to JSON-serializable format.

        Args:
            obj: Object that may contain Langflow types

        Returns:
            JSON-serializable version of the object
        """
        if hasattr(obj, "__langflow_type__"):
            # This is already a serialized Langflow object
            return obj

        # Handle Langflow Data objects
        if hasattr(obj, "__class__") and obj.__class__.__name__ == "Data":
            try:
                from langflow.schema.data import Data

                if isinstance(obj, Data):
                    # Convert Data object to serializable format
                    return self.type_converter.serialize_langflow_object(obj)
            except ImportError:
                pass

        # Handle Langflow Message objects
        if hasattr(obj, "__class__") and obj.__class__.__name__ == "Message":
            try:
                from langflow.schema.message import Message

                if isinstance(obj, Message):
                    # Convert Message object to serializable format
                    return self.type_converter.serialize_langflow_object(obj)
            except ImportError:
                pass

        # Handle lists containing Langflow objects
        if isinstance(obj, list):
            return [self._serialize_langflow_objects(item) for item in obj]

        # Handle dictionaries containing Langflow objects
        if isinstance(obj, dict):
            return {
                key: self._serialize_langflow_objects(value)
                for key, value in obj.items()
            }

        # Return unchanged for primitive types and unknown objects
        return obj

    async def _compile_component(self, blob_data: dict[str, Any]) -> dict[str, Any]:
        """Compile a component from enhanced blob data into an executable definition."""
        component_type = blob_data.get("component_type", "Unknown")
        code = blob_data.get("code", "")
        template = blob_data.get("template", {})
        outputs = blob_data.get("outputs", [])
        selected_output = blob_data.get("selected_output")

        # Extract enhanced metadata from Phase 1 improvements
        base_classes = blob_data.get("base_classes", [])
        display_name = blob_data.get("display_name", component_type)
        description = blob_data.get("description", "")
        documentation = blob_data.get("documentation", "")
        metadata = blob_data.get("metadata", {})
        field_order = blob_data.get("field_order", [])
        icon = blob_data.get("icon", "")

        # Compiling component with metadata

        # Create execution environment with enhanced context
        self._create_execution_environment()

        if not code or not code.strip():
            raise ExecutionError(
                f"No code found for component {component_type}. "
                f"All executable components should have custom code."
            )

        try:
            from langflow.custom.eval import eval_custom_component_code

            component_class = eval_custom_component_code(code)
        except Exception as e:
            raise ExecutionError(
                f"Failed to evaluate component code for {component_type}: {e}"
            ) from e

        if not component_class:
            raise ExecutionError(
                f"eval_custom_component_code returned None for {component_type}"
            )

        # Determine execution method with enhanced logic
        execution_method = self._determine_execution_method(outputs, selected_output)
        if not execution_method:
            raise ExecutionError(f"No execution method found for {component_type}")

        return {
            "class": component_class,
            "template": template,
            "execution_method": execution_method,
            "component_type": component_type,
            # Enhanced metadata for better component execution
            "base_classes": base_classes,
            "display_name": display_name,
            "description": description,
            "documentation": documentation,
            "metadata": metadata,
            "field_order": field_order,
            "icon": icon,
        }

    async def _execute_with_precompiled_components(
        self, input_data: dict[str, Any]
    ) -> dict[str, Any]:
        """Execute using pre-compiled components - no context needed."""
        return await self._execute_single_component_precompiled(input_data)

    async def _execute_single_component_precompiled(
        self, input_data: dict[str, Any]
    ) -> dict[str, Any]:
        """Execute a single component using pre-compiled data."""
        blob_id = input_data.get("blob_id")
        if not blob_id:
            raise ExecutionError("No blob_id provided")

        if blob_id not in self.compiled_components:
            raise ExecutionError(f"Component {blob_id} not pre-compiled")

        compiled_component = self.compiled_components[blob_id]
        runtime_inputs = input_data.get("input", {})

        # Execute the component
        result = await self._execute_compiled_component(
            compiled_component, runtime_inputs
        )

        return {"result": self.type_converter.serialize_langflow_object(result)}

    async def _execute_compiled_component(
        self, compiled_component: dict[str, Any], runtime_inputs: dict[str, Any]
    ) -> Any:
        """Execute a pre-compiled component instance."""
        component_class = compiled_component["class"]
        template = compiled_component["template"]
        execution_method = compiled_component["execution_method"]
        component_type = compiled_component["component_type"]

        # Create component instance
        try:
            component_instance = component_class()
        except Exception as e:
            raise ExecutionError(f"Failed to instantiate {component_type}: {e}") from e

        # Prepare parameters
        component_parameters = await self._prepare_component_parameters(
            template, runtime_inputs
        )

        # Apply component input defaults before configuring
        component_parameters = self._apply_component_input_defaults(
            component_instance, component_parameters
        )
        session_id = component_parameters.get("session_id", "default_session")
        component_instance._session_id = session_id

        # Set up graph object for components that use self.graph.session_id (like Agent)
        if not hasattr(component_instance, "graph"):
            # Create a simple graph object with session_id
            class GraphContext:
                def __init__(self, session_id: str):
                    self.session_id = session_id

            component_instance.graph = GraphContext(session_id)

        resolved_parameters = component_parameters

        # Configure component
        if hasattr(component_instance, "set_attributes"):
            component_instance._parameters = resolved_parameters
            component_instance.set_attributes(resolved_parameters)

        # Component configuration prepared for execution

        # Execute component method
        if not hasattr(component_instance, execution_method):
            available = [m for m in dir(component_instance) if not m.startswith("_")]
            raise ExecutionError(
                f"Method {execution_method} not found in {component_type}. "
                f"Available: {available}"
            )

        method = getattr(component_instance, execution_method)

        try:
            if inspect.iscoroutinefunction(method):
                # Execute all async methods directly - including real Agent execution
                result = await method()
            else:
                # Handle sync methods safely
                result = await self._execute_sync_method_safely(method, component_type)

            # Serialize Langflow objects to JSON-compatible format before returning
            return self._serialize_langflow_objects(result)
        except Exception as e:
            raise ExecutionError(f"Failed to execute {execution_method}: {e}") from e

    def _create_execution_environment(self) -> dict[str, Any]:
        """Create safe execution environment with Langflow imports."""
        exec_globals = globals().copy()
        exec_globals["sys"] = sys

        # Import common Langflow types
        try:
            from langflow.custom.custom_component.component import Component
            from langflow.schema.data import Data
            from langflow.schema.dataframe import DataFrame
            from langflow.schema.message import Message

            exec_globals.update(
                {
                    "Message": Message,
                    "Data": Data,
                    "DataFrame": DataFrame,
                    "Component": Component,
                }
            )
        except ImportError as e:
            raise ExecutionError(f"Failed to import Langflow components: {e}") from e

        return exec_globals

    async def _prepare_component_parameters(
        self, template: dict[str, Any], runtime_inputs: dict[str, Any]
    ) -> dict[str, Any]:
        """Prepare component parameters from template and runtime inputs."""

        component_parameters = {}

        # Process template parameters
        for key, field_def in template.items():
            if isinstance(field_def, dict) and "value" in field_def:
                value = field_def["value"]
                component_parameters[key] = value

        # Add runtime inputs (these override template values)
        for key, value in runtime_inputs.items():
            # Convert Stepflow values back to Langflow types if needed
            if isinstance(value, dict) and (
                "__langflow_type__" in value
                or "__class_name__" in value
                or "__tool_wrapper__" in value
            ):
                actual_value = self.type_converter.deserialize_to_langflow_type(value)
                component_parameters[key] = actual_value
            elif isinstance(value, list):
                # Use the type converter's recursive deserialization for lists
                # This will handle tool wrappers and other complex objects
                actual_value = self.type_converter.deserialize_to_langflow_type(value)

                # Check if we should convert list of Data objects to DataFrame
                if actual_value and all(
                    hasattr(item, "__class__") and item.__class__.__name__ == "Data"
                    for item in actual_value
                    if hasattr(item, "__class__")
                ):
                    # Check if component expects DataFrame input by looking at template
                    template_field = template.get(key, {})
                    input_types = template_field.get("input_types", [])

                    if "DataFrame" in input_types:
                        # Convert list of Data objects to DataFrame
                        try:
                            from langflow.schema.dataframe import DataFrame

                            dataframe = DataFrame(data=actual_value)
                            component_parameters[key] = dataframe
                        except Exception:
                            # If DataFrame conversion fails, keep as list
                            component_parameters[key] = actual_value
                    else:
                        component_parameters[key] = actual_value
                else:
                    component_parameters[key] = actual_value
            else:
                component_parameters[key] = value

        # Defaults will be applied from component inputs definition during execution
        # This respects the actual component specification rather than hardcoding

        return component_parameters

    def _apply_component_input_defaults(
        self, component_instance: Any, component_parameters: dict[str, Any]
    ) -> dict[str, Any]:
        """Apply component input defaults for missing parameters.

        Args:
            component_instance: Instantiated component
            component_parameters: Current parameters

        Returns:
            Parameters with defaults applied from component inputs definition
        """
        # Extract defaults from component inputs if available
        if hasattr(component_instance, "inputs"):
            for input_field in component_instance.inputs:
                if hasattr(input_field, "name") and hasattr(input_field, "value"):
                    param_name = input_field.name
                    default_value = input_field.value

                    # Only set default if missing and default is not None/empty
                    if (
                        param_name not in component_parameters
                        and default_value is not None
                        and default_value != ""
                    ):
                        component_parameters[param_name] = default_value
                        # Applied default parameter value

        return component_parameters

    def _determine_execution_method(
        self, outputs: list, selected_output: str | None
    ) -> str | None:
        """Determine execution method from outputs metadata."""
        if selected_output:
            for output in outputs:
                if output.get("name") == selected_output:
                    method = output.get("method")
                    if isinstance(method, str) and method:
                        return method

        # Fallback to first output's method
        if outputs:
            method = outputs[0].get("method")
            if isinstance(method, str) and method:
                return method

        return None

    async def _execute_sync_method_safely(self, method, component_type: str):
        """Execute sync method safely in async context - real execution only."""
        # Execute all sync methods directly - let components handle their execution
        return method()
