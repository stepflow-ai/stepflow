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
import os
import sys
from typing import Any

from stepflow_py import StepflowContext

from ..utils.errors import ExecutionError
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
                    print(f"ERROR UDFExecutor: Failed to load blob {blob_id}: {e}")
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

        # Tool sequence blob IDs
        if "tool_sequence_config" in input_data:
            sequence_config = input_data["tool_sequence_config"]

            # Tool blob IDs
            for tool_config in sequence_config.get("tools", []):
                if "blob_id" in tool_config:
                    blob_id = tool_config["blob_id"]
                    # Handle external references
                    if blob_id in input_data and isinstance(input_data[blob_id], str):
                        blob_ids.add(input_data[blob_id])
                    else:
                        blob_ids.add(blob_id)

            # Agent blob ID
            agent_config = sequence_config.get("agent", {})
            if "blob_id" in agent_config:
                blob_id = agent_config["blob_id"]
                # Handle external references
                if blob_id in input_data and isinstance(input_data[blob_id], str):
                    blob_ids.add(input_data[blob_id])
                else:
                    blob_ids.add(blob_id)

        return blob_ids

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

        if not code:
            raise ExecutionError(f"No code found for component {component_type}")

        print(
            f"DEBUG UDFExecutor: Compiling {display_name} ({component_type}) with "
            f"base_classes={base_classes}"
        )

        # Create execution environment with enhanced context
        exec_globals = self._create_execution_environment()

        # Strategy: Prefer custom code when available, fallback to Langflow imports

        if (
            code
            and code.strip()
            and code.strip() != "pass"
            and "# Will be loaded dynamically" not in code
        ):
            # Custom code available - use it (user's modifications/customizations)
            print(
                f"DEBUG UDFExecutor: Using custom component code for {component_type}"
            )
            try:
                exec(code, exec_globals)
                print(
                    f"DEBUG UDFExecutor: Successfully executed custom code "
                    f"for {component_type}"
                )
            except Exception as e:
                raise ExecutionError(
                    f"Failed to execute component code for {component_type}: {e}"
                ) from e

            # Find component class with enhanced detection
            component_class = self._find_component_class(
                exec_globals, component_type, base_classes, metadata
            )
            if not component_class:
                available_classes = [
                    name
                    for name, obj in exec_globals.items()
                    if isinstance(obj, type) and not name.startswith("_")
                ]
                raise ExecutionError(
                    f"Component class {component_type} not found. "
                    f"Available classes: {available_classes}"
                )
        else:
            # No custom code - use standard Langflow component imports
            print(
                f"DEBUG UDFExecutor: No custom code, loading standard "
                f"Langflow component {component_type}"
            )
            component_class = self._try_dynamic_builtin_loading(
                component_type, base_classes, metadata
            )
            if not component_class:
                raise ExecutionError(
                    f"Could not load standard Langflow component {component_type}"
                )
            print(
                f"DEBUG UDFExecutor: Using standard Langflow component "
                f"{component_class.__name__}"
            )

        # Determine execution method with enhanced logic
        execution_method = self._determine_execution_method(outputs, selected_output)
        if not execution_method:
            execution_method = self._infer_execution_method_from_component_class(
                component_class
            )
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

        # Check if this is a tool sequence execution
        if self._is_tool_sequence_execution(input_data):
            return await self._execute_tool_sequence_precompiled(input_data)
        else:
            return await self._execute_single_component_precompiled(input_data)

    def _is_tool_sequence_execution(self, input_data: dict[str, Any]) -> bool:
        """Check if this is a tool sequence execution pattern."""
        return "tool_sequence_config" in input_data

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

    async def _execute_tool_sequence_precompiled(
        self, input_data: dict[str, Any]
    ) -> dict[str, Any]:
        """Execute tool sequence using pre-compiled components."""
        sequence_config = input_data["tool_sequence_config"]
        tool_configs = sequence_config["tools"]
        agent_config = sequence_config["agent"]
        runtime_inputs = input_data.get("input", {})

        # Execute each tool using pre-compiled components
        tool_results = []
        for tool_config in tool_configs:
            blob_id = tool_config["blob_id"]

            # Handle external references
            if blob_id in input_data and isinstance(input_data[blob_id], str):
                actual_blob_id = input_data[blob_id]
            else:
                actual_blob_id = blob_id

            if actual_blob_id not in self.compiled_components:
                raise ExecutionError(
                    f"Tool component {actual_blob_id} not pre-compiled"
                )

            compiled_tool = self.compiled_components[actual_blob_id]
            tool_inputs = tool_config.get("inputs", {})

            tool_result = await self._execute_compiled_component(
                compiled_tool, tool_inputs
            )

            # Enhance tool result with metadata for Agent validation
            tool_name = tool_config.get("component_type", "unknown").lower()
            if hasattr(tool_result, "data") and isinstance(tool_result.data, dict):
                tool_result.data["name"] = tool_name
            elif hasattr(tool_result, "__dict__"):
                # Only set name if object supports attribute assignment
                tool_result.name = tool_name
            else:
                # For immutable objects (list, float, str, etc.), wrap in a simple container
                print(
                    f"DEBUG UDFExecutor: Tool result is {type(tool_result)}, wrapping with name"
                )

                class NamedToolResult:
                    def __init__(self, result, name):
                        self.result = result
                        self.name = name
                        # Preserve common attributes for tool usage
                        if hasattr(result, "data"):
                            self.data = result.data
                        if hasattr(result, "text"):
                            self.text = result.text

                tool_result = NamedToolResult(tool_result, tool_name)

            tool_results.append(tool_result)

        # Execute agent using pre-compiled components
        agent_blob_id = agent_config["blob_id"]

        # Handle external references
        if agent_blob_id in input_data and isinstance(input_data[agent_blob_id], str):
            actual_agent_blob_id = input_data[agent_blob_id]
        else:
            actual_agent_blob_id = agent_blob_id

        if actual_agent_blob_id not in self.compiled_components:
            raise ExecutionError(
                f"Agent component {actual_agent_blob_id} not pre-compiled"
            )

        compiled_agent = self.compiled_components[actual_agent_blob_id]

        # Convert tools to BaseTool instances
        basetools = self._convert_tools_to_basetools(tool_results)

        # Prepare agent inputs
        agent_inputs = runtime_inputs.copy()
        agent_inputs["tools"] = basetools

        # Execute agent
        agent_result = await self._execute_compiled_component(
            compiled_agent, agent_inputs
        )

        return {"result": self.type_converter.serialize_langflow_object(agent_result)}

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

        # Let all components handle their own setup - no special cases needed

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
        # TODO: Also consider setting `flow_id`, `user_id`, `flow_name`.

        # Configure component
        if hasattr(component_instance, "set_attributes"):
            component_instance._parameters = component_parameters
            component_instance.set_attributes(component_parameters)

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
                return await method()
            else:
                # Handle sync methods safely
                return await self._execute_sync_method_safely(method, component_type)
        except Exception as e:
            raise ExecutionError(f"Failed to execute {execution_method}: {e}") from e

    def _convert_tools_to_basetools(self, tool_results: list[Any]) -> list[Any]:
        """Convert Langflow tool objects to BaseTool implementations for Agent."""
        from typing import Any as AnyType

        from langchain_core.tools import BaseTool

        class LangflowBaseTool(BaseTool):
            """BaseTool wrapper for Langflow component results."""

            langflow_result: AnyType

            def __init__(self, name: str, description: str, langflow_result: AnyType):
                super().__init__(
                    name=name,
                    description=description,
                    langflow_result=langflow_result,
                )

            def _run(self, *args: AnyType, **kwargs: AnyType) -> str:
                """Execute the tool with the given input."""
                try:
                    # Extract the input from args - LangChain passes it as the first argument
                    input_str = args[0] if args else ""

                    # Handle our wrapped tool results
                    result_to_use = self.langflow_result
                    if hasattr(self.langflow_result, "result"):
                        # This is our NamedToolResult wrapper
                        result_to_use = self.langflow_result.result

                    if hasattr(result_to_use, "data"):
                        import json

                        return json.dumps(result_to_use.data)
                    elif hasattr(result_to_use, "to_dict"):
                        import json

                        return json.dumps(result_to_use.to_dict())
                    else:
                        return str(result_to_use)
                except Exception as e:
                    return f"Tool execution error: {str(e)}"

            async def _arun(self, *args: AnyType, **kwargs: AnyType) -> str:
                """Async version of _run."""
                # Extract the input from args - LangChain passes it as the first argument
                input_str = args[0] if args else ""
                return self._run(input_str, **kwargs)

        basetools = []
        for tool in tool_results:
            tool_name = getattr(tool, "name", "unknown_tool")
            tool_description = f"Execute {tool_name} tool with given input"

            basetool = LangflowBaseTool(
                name=tool_name, description=tool_description, langflow_result=tool
            )
            basetools.append(basetool)

        return basetools

    def _create_execution_environment(self) -> dict[str, Any]:
        """Create safe execution environment with Langflow imports."""
        exec_globals = globals().copy()
        exec_globals["os"] = os
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

    def _find_component_class(
        self,
        exec_globals: dict[str, Any],
        component_type: str,
        base_classes: list[str] = None,
        metadata: dict[str, Any] = None,
    ) -> type | None:
        """Find component class in execution environment with enhanced detection."""
        # Direct match first
        component_class = exec_globals.get(component_type)
        if component_class and isinstance(component_class, type):
            return component_class  # type: ignore[no-any-return]

        # Search with different matching strategies
        component_type_lower = component_type.lower()
        for name, obj in exec_globals.items():
            if not isinstance(obj, type):
                continue

            name_lower = name.lower()
            # Exact lowercase match
            if name_lower == component_type_lower:
                return obj
            # Component suffix match
            if name_lower == component_type_lower + "component":
                return obj
            # Component prefix match
            if (
                name_lower.endswith("component")
                and name_lower[:-9] == component_type_lower
            ):
                return obj

        # Enhanced search using base_classes information from Phase 1
        if base_classes:
            # Look for components that might inherit from specific Langflow base classes
            from langflow.custom.custom_component.component import Component

            for name, obj in exec_globals.items():
                if isinstance(obj, type) and issubclass(obj, Component):
                    # This is a Langflow component, likely what we want
                    return obj

        # Try dynamic loading for built-in components if no class found in exec_globals
        component_class = self._try_dynamic_builtin_loading(
            component_type, base_classes, metadata or {}
        )
        if component_class:
            return component_class

        return None

    def _try_dynamic_builtin_loading(
        self, component_type: str, base_classes: list[str] = None, metadata: dict = None
    ) -> type | None:
        """Try to dynamically load built-in Langflow components using metadata.

        Args:
            component_type: Component type name
            base_classes: Base class information for the component
            metadata: Component metadata containing module path

        Returns:
            Component class if found, None otherwise
        """
        print(
            f"DEBUG UDFExecutor: Attempting dynamic loading of "
            f"built-in {component_type}"
        )

        # First try using the module path from component metadata (most accurate)
        if metadata and "module" in metadata:
            module_path = metadata["module"]
            try:
                import importlib

                module = importlib.import_module(module_path)

                # Extract class name from module path or use component_type
                class_name = module_path.split(".")[-1]
                if hasattr(module, class_name):
                    component_class = getattr(module, class_name)
                    if isinstance(component_class, type):
                        print(
                            f"DEBUG UDFExecutor: Found {component_type} "
                            f"at {module_path}.{class_name}"
                        )
                        return component_class

                # Fallback: try component_type as class name
                if hasattr(module, component_type):
                    component_class = getattr(module, component_type)
                    if isinstance(component_class, type):
                        print(
                            f"DEBUG UDFExecutor: Found {component_type} "
                            f"at {module_path}.{component_type}"
                        )
                        return component_class

            except ImportError as e:
                print(
                    "DEBUG UDFExecutor: Failed to import from metadata module"
                    f"{module_path}: {e}"
                )

        # Fallback: Common built-in component import patterns
        builtin_imports = [
            # Model components - Current Langflow 1.5.0 location
            "langflow.components.models.language_model",  # LanguageModelComponent
            # Input/Output components - Current Langflow 1.5.0 locations
            "langflow.components.input_output.chat",  # ChatInput
            "langflow.components.input_output.chat_output",  # ChatOutput
            # Other common component locations
            f"langflow.components.prompts.{component_type.lower()}",
            f"langflow.components.helpers.{component_type.lower()}",
            f"langflow.components.data.{component_type.lower()}",
            f"langflow.components.models.{component_type.lower()}",
            # Legacy paths (for backward compatibility)
            "langflow.base.models.model",
            "langflow.components.models.openai",
            "langflow.components.models.anthropic",
        ]

        for import_path in builtin_imports:
            try:
                import importlib

                module = importlib.import_module(import_path)

                # Try multiple class name variations
                class_names = [component_type, f"{component_type}Component"]
                for class_name in class_names:
                    if hasattr(module, class_name):
                        component_class = getattr(module, class_name)
                        if isinstance(component_class, type):
                            return component_class

            except ImportError:
                continue

        return None

    async def _prepare_component_parameters(
        self, template: dict[str, Any], runtime_inputs: dict[str, Any]
    ) -> dict[str, Any]:
        """Prepare component parameters from template and runtime inputs."""
        component_parameters = {}

        # Process template parameters
        for key, field_def in template.items():
            if isinstance(field_def, dict) and "value" in field_def:
                value = field_def["value"]

                # Handle environment variables
                env_var = self._determine_environment_variable(key, value, field_def)
                if env_var:
                    actual_value = os.getenv(env_var)
                    if actual_value:
                        value = actual_value
                    elif field_def.get("_input_type") == "SecretStrInput":
                        print(
                            f"⚠️  Environment variable {env_var} not found for {key}",
                            file=sys.stderr,
                        )

                component_parameters[key] = value

            # Handle complex configuration objects (embedded components)
            elif key.startswith("_embedding_config_"):
                embedding_field = key.replace("_embedding_config_", "")
                embedding_config = field_def.get("value", {})

                if embedding_config.get("component_type") == "OpenAIEmbeddings":
                    try:
                        from langchain_openai import OpenAIEmbeddings

                        embedding_params = embedding_config.get("config", {})

                        # Handle API key from environment
                        if "api_key" in embedding_params:
                            api_key_value = embedding_params["api_key"]
                            if (
                                isinstance(api_key_value, str)
                                and api_key_value.startswith("${")
                                and api_key_value.endswith("}")
                            ):
                                env_var = api_key_value[2:-1]
                                actual_api_key = os.getenv(env_var)
                                if actual_api_key:
                                    embedding_params["api_key"] = actual_api_key

                        embeddings = OpenAIEmbeddings(**embedding_params)
                        component_parameters[embedding_field] = embeddings
                    except Exception:
                        pass

        # Add runtime inputs (these override template values)
        for key, value in runtime_inputs.items():
            # Convert Stepflow values back to Langflow types if needed
            if isinstance(value, dict) and "__langflow_type__" in value:
                actual_value = self.type_converter.deserialize_to_langflow_type(value)
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
                        import sys

                        print(
                            f"Applied default {param_name}={default_value}",
                            file=sys.stderr,
                        )

        return component_parameters

    def _determine_environment_variable(
        self, field_name: str, field_value: Any, field_config: dict[str, Any]
    ) -> str | None:
        """Determine environment variable name for a field."""
        # Template string like "${OPENAI_API_KEY}"
        if (
            isinstance(field_value, str)
            and field_value.startswith("${")
            and field_value.endswith("}")
        ):
            return field_value[2:-1]

        # Direct environment variable name (like "ANTHROPIC_API_KEY")
        if (
            isinstance(field_value, str)
            and field_value.isupper()
            and "_" in field_value
            and field_value.endswith("_API_KEY")
        ):
            return field_value

        # Secret input fields
        if field_config.get("_input_type") == "SecretStrInput":
            if field_name == "api_key":
                return "OPENAI_API_KEY"
            elif "openai" in field_name.lower():
                return "OPENAI_API_KEY"
            elif "anthropic" in field_name.lower():
                return "ANTHROPIC_API_KEY"
            else:
                return field_name.upper()

        return None

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

    def _infer_execution_method_from_component_class(
        self, component_class: type
    ) -> str | None:
        """Infer execution method from component class."""
        # Try to create a temporary instance to check available methods
        try:
            temp_instance = component_class()
            return self._infer_execution_method_from_component(temp_instance)
        except Exception:
            # Fall back to static analysis if instantiation fails
            common_methods = [
                "build_prompt",
                "process_message",
                "build_model",
                "execute",
                "process",
                "build",
                "embed_query",
                "run",
                "invoke",
            ]

            for method_name in common_methods:
                if hasattr(component_class, method_name):
                    return method_name

            return None

    def _infer_execution_method_from_component(self, component_instance) -> str | None:
        """Infer execution method by examining the component instance."""
        # Check if the component has an outputs attribute we can examine
        if hasattr(component_instance, "outputs"):
            outputs = getattr(component_instance, "outputs", [])
            if outputs and hasattr(outputs[0], "method"):
                method = outputs[0].method
                if isinstance(method, str):
                    return method

        # Look for common Langflow component method patterns
        common_methods = [
            "build_prompt",
            "process_message",
            "build_model",
            "execute",
            "process",
            "build",
            "embed_query",
            "run",
            "invoke",
        ]

        for method_name in common_methods:
            if hasattr(component_instance, method_name):
                return method_name

        # If component has __call__, use that
        if callable(component_instance):
            return "__call__"

        return None

    async def _execute_sync_method_safely(self, method, component_type: str):
        """Execute sync method safely in async context - real execution only."""
        # Execute all sync methods directly - let components handle their execution
        return method()
