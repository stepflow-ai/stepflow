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

"""Process individual Langflow nodes into Stepflow steps."""

from typing import Any

from stepflow_py import FlowBuilder, Value

from ..exceptions import ConversionError
from .schema_mapper import SchemaMapper


class NodeProcessor:
    """Processes individual Langflow nodes into Stepflow steps."""

    def __init__(self):
        """Initialize node processor."""
        self.schema_mapper = SchemaMapper()

    def process_node(
        self,
        node: dict[str, Any],
        dependencies: dict[str, list[str]],
        all_nodes: list[dict[str, Any]],
        builder: FlowBuilder,
        node_output_refs: dict[str, Any],
        field_mapping: dict[str, dict[str, str]] = None,
    ) -> Any | None:
        """Process a Langflow node using flow builder architecture.

        Args:
            node: Langflow node object
            dependencies: Dependency graph for all nodes
            all_nodes: All nodes in the workflow
            builder: FlowBuilder instance
            node_output_refs: Mapping of node IDs to their output references
            field_mapping: Mapping of target nodes to their input field names
                from edges

        Returns:
            Output reference for this node, or None if node should be skipped
        """
        try:
            node_id = node.get("id")
            if not node_id:
                raise ConversionError("Node missing ID")

            # Check if this is a note or documentation node that should be skipped
            node_type = node.get("type", "")
            if node_type == "noteNode":
                # This is a React Flow note node - skip it entirely
                return None

            node_data = node.get("data", {})
            component_type = node_data.get("type", "")

            # Skip nodes without a valid component type (documentation nodes, etc.)
            if not component_type or component_type.strip() == "":
                return None

            # Skip tool dependency nodes that will be handled by Agent tool sequences
            if self._is_tool_dependency_of_agent(node_id, all_nodes, dependencies):
                print(f"({component_type}) - handled by Agent tool sequence")
                return None

            # Generate step ID (clean up for Stepflow)
            step_id = self._generate_step_id(node_id, component_type)

            # Get node structure info for routing decisions
            node_info = node_data.get("node", {})
            template = node_info.get("template", {})

            # Handle ChatInput/ChatOutput as I/O connection points (not processing
            # steps)
            if component_type == "ChatInput":
                # ChatInput returns a reference to workflow input directly
                return Value.input.add_path("message")
            elif component_type == "ChatOutput":
                # ChatOutput depends on another node - return that node's output
                # reference
                dependency_node_ids = dependencies.get(node_id, [])
                if dependency_node_ids and dependency_node_ids[0] in node_output_refs:
                    return node_output_refs[dependency_node_ids[0]]
                else:
                    # ChatOutput with no dependencies - return input passthrough
                    return Value.input.add_path("message")

            # For all other components, create a step and return a reference to it
            node_info = node_data.get("node", {})
            template = node_info.get("template", {})
            custom_code = template.get("code", {}).get("value", "")

            # Check for embedded configuration marker
            has_embedded_config = template.get("_has_embedded_config", {}).get(
                "value", False
            )

            # Determine component path and inputs based on available information
            if self._is_agent_with_tools(
                node, dependencies.get(node_id, []), all_nodes
            ):
                # Agent with tool dependencies - use enhanced UDF machinery
                component_path = "/langflow/udf_executor"
                step_input = self._create_tool_sequence_config(
                    node,
                    dependencies.get(node_id, []),
                    all_nodes,
                    builder,
                    node_output_refs,
                    field_mapping,
                )
            elif custom_code:
                # Any component with code - use UDF executor for real execution
                import sys

                print(f"Routing {component_type} to UDF executor", file=sys.stderr)
                component_path = "/langflow/udf_executor"

                # First create a blob step for the UDF code using auto ID generation
                blob_data = self._prepare_udf_blob(node, component_type)

                blob_step_id = f"{step_id}_blob"
                blob_step_handle = builder.add_step(
                    id=blob_step_id,
                    component="/builtin/put_blob",
                    input_data={"data": blob_data, "blob_type": "data"},
                )

                # Now create the UDF executor step that uses the blob
                step_input = {
                    "blob_id": Value.step(blob_step_handle.id, "blob_id"),
                    "input": self._extract_runtime_inputs_for_builder(
                        node,
                        dependencies.get(node_id, []),
                        node_output_refs,
                        field_mapping,
                    ),
                }
            elif has_embedded_config and component_type in [
                "AstraDB",
                "VectorStore",
                "Chroma",
                "FAISS",
                "Pinecone",
            ]:
                # Vector store with embedded configuration - use standalone server
                # TODO: Phase 4 will route these through UDF executor too
                print(
                    f"Routing {component_type} config to standalone server (temporary)"
                )
                component_path = f"/langflow/{component_type}"
                step_input = self._extract_component_inputs_for_builder(
                    node, dependencies.get(node_id, []), all_nodes, node_output_refs
                )
            else:
                # Components without custom code - create blob for built-in component
                import sys

                print(
                    f"Routing built-in {component_type} through UDF executor",
                    file=sys.stderr,
                )
                component_path = "/langflow/udf_executor"

                # Create blob for built-in component using node structure
                blob_data = self._prepare_builtin_component_blob(node, component_type)

                blob_step_id = f"{step_id}_blob"
                blob_step_handle = builder.add_step(
                    id=blob_step_id,
                    component="/builtin/put_blob",
                    input_data={"data": blob_data, "blob_type": "data"},
                )

                # Create UDF executor step
                step_input = {
                    "blob_id": Value.step(blob_step_handle.id, "blob_id"),
                    "input": self._extract_runtime_inputs_for_builder(
                        node,
                        dependencies.get(node_id, []),
                        node_output_refs,
                        field_mapping,
                    ),
                }

            # Add step to builder with proper ID and component path
            step_id = self._generate_step_id(node_id, component_type)
            step_handle = builder.add_step(
                id=step_id,
                component=component_path,
                input_data=step_input,
            )

            # Return a reference to this step's output
            return Value.step(step_handle.id, "result")

        except Exception as e:
            import traceback

            print(
                f"Full traceback for node {node.get('id', 'unknown')}: "
                f"{traceback.format_exc()}"
            )
            raise ConversionError(
                f"Error processing node {node.get('id', 'unknown')}: {e}"
            ) from e

    def _generate_step_id(self, node_id: str, component_type: str) -> str:
        """Generate a clean step ID from node ID and type.

        Args:
            node_id: Original Langflow node ID
            component_type: Component type name

        Returns:
            Clean step ID suitable for Stepflow
        """
        # Always use the full node_id to ensure uniqueness
        # For any node_id with a suffix, keep it to ensure uniqueness
        base_id = node_id.lower()

        # Always use langflow prefix with the full base_id to guarantee uniqueness
        return f"langflow_{base_id}"

    def _prepare_udf_blob(
        self, node: dict[str, Any], component_type: str
    ) -> dict[str, Any]:
        """Prepare enhanced UDF blob data for component execution.

        Args:
            node: Langflow node object
            component_type: Component type name

        Returns:
            Enhanced UDF blob data with complete component information
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})

        # Extract component code
        template = node_info.get("template", {})
        code = template.get("code", {}).get("value", "")

        if not code:
            raise ConversionError(f"No code found for component {component_type}")

        # Extract comprehensive component metadata
        outputs = node_data.get("outputs", [])
        node_outputs = node_info.get("outputs", [])

        # Use node outputs if available (more complete), fallback to data outputs
        final_outputs = node_outputs if node_outputs else outputs

        selected_output = None
        if final_outputs:
            # For now, use the first output
            selected_output = final_outputs[0].get("name")

        # Extract additional component metadata from node_info
        base_classes = node_info.get("base_classes", [])
        display_name = node_info.get("display_name", component_type)
        description = node_info.get("description", "")
        documentation = node_info.get("documentation", "")
        metadata = node_info.get("metadata", {})

        # Extract field order for proper component initialization
        field_order = node_info.get("field_order", [])

        # Extract component icon and UI information
        icon = node_info.get("icon", "")

        # Prepare template (remove code field to avoid duplication)
        prepared_template: dict[str, Any] = {}
        embedding_config_count = 0
        for field_name, field_config in template.items():
            if field_name != "code":
                prepared_template[field_name] = field_config
                if field_name.startswith("_embedding_config_"):
                    embedding_config_count += 1

        # Debug: Log embedded configuration preservation
        if embedding_config_count > 0:
            print(
                f"Preserved {embedding_config_count} embedding configs for "
                f"{component_type}"
            )
        else:
            print(f"No embedding configs found for {component_type}")

        # Return enhanced blob data with complete component information
        blob_data = {
            "code": code,
            "template": prepared_template,
            "component_type": component_type,
            "outputs": final_outputs,
            "selected_output": selected_output,
            # Enhanced metadata for real component execution
            "base_classes": base_classes,
            "display_name": display_name,
            "description": description,
            "documentation": documentation,
            "metadata": metadata,
            "field_order": field_order,
            "icon": icon,
        }

        import sys

        print(
            f"Enhanced blob for {component_type}: base_classes={base_classes}, "
            f"display_name={display_name}",
            file=sys.stderr,
        )

        return blob_data

    def _prepare_builtin_component_blob(
        self, node: dict[str, Any], component_type: str
    ) -> dict[str, Any]:
        """Prepare blob data for built-in Langflow components without custom code.

        Args:
            node: Langflow node object
            component_type: Component type name

        Returns:
            Blob data with component import information for built-in components
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})

        # Extract component metadata
        template = node_info.get("template", {})
        outputs = node_data.get("outputs", [])
        node_outputs = node_info.get("outputs", [])

        # Use node outputs if available, fallback to data outputs
        final_outputs = node_outputs if node_outputs else outputs

        selected_output = None
        if final_outputs:
            selected_output = final_outputs[0].get("name")

        # Extract metadata from node_info
        base_classes = node_info.get("base_classes", [])
        display_name = node_info.get("display_name", component_type)
        description = node_info.get("description", "")
        documentation = node_info.get("documentation", "")
        metadata = node_info.get("metadata", {})
        field_order = node_info.get("field_order", [])
        icon = node_info.get("icon", "")

        # For built-in components, generate import code based on metadata
        module_path = metadata.get("module", "")
        if module_path:
            # Generate import code for the built-in component
            module_parts = module_path.split(".")
            class_name = module_parts[-1] if module_parts else component_type

            builtin_code = f"""# Built-in Langflow component: {component_type}
from {module_path} import {class_name}

# Component class is imported from Langflow
# This allows the UDF executor to load and execute the real built-in component
{class_name} = {class_name}
"""
        else:
            # Fallback for components without module metadata
            builtin_code = f"""# Built-in Langflow component: {component_type}
# Will be loaded dynamically by UDF executor based on component_type
pass
"""

        import sys

        print(
            f"Created built-in blob for {component_type} from module: {module_path}",
            file=sys.stderr,
        )

        return {
            "code": builtin_code,
            "template": template,
            "component_type": component_type,
            "outputs": final_outputs,
            "selected_output": selected_output,
            "base_classes": base_classes,
            "display_name": display_name,
            "description": description,
            "documentation": documentation,
            "metadata": metadata,
            "field_order": field_order,
            "icon": icon,
            "is_builtin": True,  # Mark as built-in component
        }

    def _extract_runtime_inputs(
        self, node: dict[str, Any], dependency_node_ids: list[str]
    ) -> dict[str, Any]:
        """Extract runtime inputs that will come from other workflow steps.

        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on

        Returns:
            Dict of runtime inputs
        """
        runtime_inputs: dict[str, Any] = {}

        # For now, create placeholder inputs based on dependencies
        # TODO: Map actual edge connections to specific input fields
        for i, dep_id in enumerate(dependency_node_ids):
            dep_step_id = self._generate_step_id(dep_id, "")
            runtime_inputs[f"input_{i}"] = {
                "$from": {"step": dep_step_id},
                "path": "result",
            }

        return runtime_inputs

    def _extract_component_inputs_for_builder(
        self,
        node: dict[str, Any],
        dependency_node_ids: list[str],
        all_nodes: list[dict[str, Any]],
        node_output_refs: dict[str, Any],
    ) -> dict[str, Any]:
        """Extract inputs for built-in Langflow components using flow builder
        architecture.

        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on
            all_nodes: All nodes in the workflow
            node_output_refs: Mapping of node IDs to their output references

        Returns:
            Dict of component inputs mapped to template fields
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        template = node_info.get("template", {})

        component_inputs: dict[str, Any] = {}

        # Map static template values to component inputs
        for field_name, field_config in template.items():
            if isinstance(field_config, dict):
                field_value = field_config.get("value")
                if field_value is not None and field_value != "":
                    component_inputs[field_name] = field_value
                elif field_name == "session_id" and (
                    field_value == "" or field_value is None
                ):
                    # Map empty session_id fields to workflow input
                    component_inputs[field_name] = {
                        "$from": {"workflow": "input"},
                        "path": "session_id",
                    }

        # Handle standalone components with workflow inputs
        component_type = node_data.get("type", "")
        if not dependency_node_ids and component_type == "File":
            # For standalone File components, map workflow file_path to path parameter
            # File component expects the path parameter to have file_path as a list
            component_inputs["path"] = {
                "$from": {"workflow": "input"},
                "path": "file_path",
            }

        # Map dependencies to component inputs based on edges
        if dependency_node_ids:
            dep_node_id = dependency_node_ids[0]

            # Check if we have an output reference for this dependency
            if dep_node_id in node_output_refs:
                input_reference = node_output_refs[dep_node_id]
            else:
                # Find the dependency node to check its type
                dep_node = None
                for n in all_nodes:
                    if n.get("id") == dep_node_id:
                        dep_node = n
                        break

                # Check if dependency is a ChatInput component
                if dep_node and dep_node.get("data", {}).get("type") == "ChatInput":
                    # Reference workflow input instead of missing ChatInput step
                    input_reference = Value.input("$.message")
                else:
                    # This shouldn't happen with the new architecture, but fallback
                    input_reference = Value.input("$.message")

            # Common input field names for different component types
            component_type = node_data.get("type", "")
            if component_type in ["ChatOutput", "TextOutput"]:
                component_inputs["input_value"] = input_reference
            elif component_type in ["LanguageModelComponent", "OpenAIModelComponent"]:
                component_inputs["input_value"] = input_reference
            else:
                # Generic mapping
                component_inputs["input"] = input_reference

        return component_inputs

    def _extract_runtime_inputs_for_builder(
        self,
        node: dict[str, Any],
        dependency_node_ids: list[str],
        node_output_refs: dict[str, Any],
        field_mapping: dict[str, dict[str, str]] = None,
    ) -> dict[str, Any]:
        """Extract runtime inputs for UDF components using flow builder architecture.

        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on
            node_output_refs: Mapping of node IDs to their output references
            field_mapping: Mapping of target nodes to their input field names
                from edges

        Returns:
            Dict of runtime inputs
        """
        runtime_inputs: dict[str, Any] = {}
        node_id = node.get("id")

        # Use field mapping if available, otherwise fall back to generic input names
        if field_mapping and node_id in field_mapping:
            # Map dependencies to their proper field names
            node_field_map = field_mapping[node_id]

            # Group inputs by field name to handle list fields like 'tools'
            field_inputs: dict[str, Any] = {}

            for dep_id in dependency_node_ids:
                if dep_id in node_field_map and dep_id in node_output_refs:
                    field_name = node_field_map[dep_id]

                    # Handle list fields by collecting multiple inputs
                    if field_name not in field_inputs:
                        field_inputs[field_name] = []
                    field_inputs[field_name].append(node_output_refs[dep_id])
                elif dep_id in node_output_refs:
                    # Fallback to generic name if no field mapping
                    runtime_inputs[f"input_{len(runtime_inputs)}"] = node_output_refs[
                        dep_id
                    ]

            # Convert to runtime inputs format
            for field_name, inputs in field_inputs.items():
                if len(inputs) == 1:
                    # Single input - use directly
                    runtime_inputs[field_name] = inputs[0]
                else:
                    # Multiple inputs - create a list
                    runtime_inputs[field_name] = inputs
        else:
            # Fallback to old behavior for backwards compatibility
            for i, dep_id in enumerate(dependency_node_ids):
                if dep_id in node_output_refs:
                    runtime_inputs[f"input_{i}"] = node_output_refs[dep_id]
                else:
                    # Fallback to workflow input if dependency not found
                    runtime_inputs[f"input_{i}"] = Value.input("$.message")

        # Add session_id mapping for UDF components (like Memory) that need it
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        template = node_info.get("template", {})
        node_id = node.get("id", "")
        component_type = node_data.get("type", "")

        # Check for session_id field that needs mapping
        if "session_id" in template:
            session_id_config = template["session_id"]
            if isinstance(session_id_config, dict):
                session_id_value = session_id_config.get("value")
                if session_id_value == "" or session_id_value is None:
                    runtime_inputs["session_id"] = Value.input("$.session_id")

        # Handle standalone File components with workflow input mapping
        if not dependency_node_ids and component_type == "File":
            # For standalone File components, map workflow file_path to path parameter
            # Path parameter should match Langflow's FileInput expectations
            # file_path should be a simple list of paths, not wrapped
            runtime_inputs["path"] = Value.input("$.file_path")

        return runtime_inputs

    def _extract_component_inputs(
        self,
        node: dict[str, Any],
        dependency_node_ids: list[str],
        all_nodes: list[dict[str, Any]] = None,
    ) -> dict[str, Any]:
        """Extract inputs for built-in Langflow components.

        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on

        Returns:
            Dict of component inputs mapped to template fields
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        template = node_info.get("template", {})

        component_inputs: dict[str, Any] = {}

        # Map static template values to component inputs
        for field_name, field_config in template.items():
            if isinstance(field_config, dict):
                field_value = field_config.get("value")
                if field_value is not None:
                    component_inputs[field_name] = field_value

        # Map dependencies to component inputs based on edges
        if dependency_node_ids and all_nodes:
            # Find the dependency node to check its type
            dep_node_id = dependency_node_ids[0]
            dep_node = None
            for n in all_nodes:
                if n.get("id") == dep_node_id:
                    dep_node = n
                    break

            # Check if dependency is a ChatInput component
            if dep_node and dep_node.get("data", {}).get("type") == "ChatInput":
                # Reference workflow input instead of missing ChatInput step
                input_reference = {"$from": {"workflow": "input"}, "path": "message"}
            else:
                # Reference the dependency step normally
                dep_step_id = self._generate_step_id(dep_node_id, "")
                input_reference = {"$from": {"step": dep_step_id}, "path": "result"}

            # Common input field names for different component types
            component_type = node_data.get("type", "")
            if component_type in ["ChatOutput", "TextOutput"]:
                component_inputs["input_value"] = input_reference
            elif component_type in ["LanguageModelComponent", "OpenAIModelComponent"]:
                component_inputs["input_value"] = input_reference
            else:
                # Generic mapping
                component_inputs["input"] = input_reference

        return component_inputs

    def _extract_chat_input_mapping(self, node: dict[str, Any]) -> dict[str, Any]:
        """Extract inputs for ChatInput component to map to workflow input.

        Args:
            node: Langflow ChatInput node object

        Returns:
            Dict mapping ChatInput to workflow input
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        template = node_info.get("template", {})

        # Get the sender from template, defaulting to "User"
        sender = "User"
        if "sender" in template:
            sender_config = template["sender"]
            if isinstance(sender_config, dict):
                sender = sender_config.get("value", "User")

        # Create a message from workflow input
        return {
            "message": {
                "$from": {"workflow": "input"},
                "path": "message",  # Expect workflow input to have a 'message' field
            },
            "sender": sender,
            "session_id": {
                "$from": {"workflow": "input"},
                "path": "session_id",  # Pass session_id from workflow input
            },
        }

    def _extract_chat_output_mapping(
        self, node: dict[str, Any], dependency_node_ids: list[str]
    ) -> dict[str, Any]:
        """Extract inputs for ChatOutput component to pass through data.

        Args:
            node: Langflow ChatOutput node object
            dependency_node_ids: IDs of nodes this node depends on

        Returns:
            Dict mapping ChatOutput inputs for identity pass-through
        """
        if dependency_node_ids:
            # Pass through the input from the previous step
            dep_step_id = self._generate_step_id(dependency_node_ids[0], "")
            return {"input_message": {"$from": {"step": dep_step_id}, "path": "result"}}
        else:
            # No dependencies, return empty value
            return {"input_message": None}

    def _create_chat_input_udf(self, node: dict[str, Any]) -> dict[str, Any]:
        """Create UDF blob data for ChatInput component.

        Args:
            node: Langflow ChatInput node object

        Returns:
            UDF blob data for ChatInput handling
        """
        # Create a simple ChatInput UDF that reads from workflow input
        chat_input_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import MessageTextInput, Output
from langflow.schema.message import Message

class ChatInputComponent(Component):
    display_name = "Chat Input"
    description = "Processes chat input from workflow"

    inputs = [
        MessageTextInput(name="message", display_name="Message"),
    ]

    outputs = [
        Output(display_name="Output", name="output", method="process_message")
    ]

    def process_message(self) -> Message:
        """Process the input message."""
        message_text = self.message or "No message provided"
        return Message(
            text=str(message_text),
            sender="User",
            sender_name="User"
        )
'''

        return {
            "code": chat_input_code,
            "component_type": "ChatInputComponent",
            "template": {
                "message": {"type": "str", "value": "", "info": "Message text input"}
            },
            "outputs": [
                {"name": "output", "method": "process_message", "types": ["Message"]}
            ],
            "selected_output": "output",
        }

    def _create_chat_output_udf(self, node: dict[str, Any]) -> dict[str, Any]:
        """Create UDF blob data for ChatOutput component.

        Args:
            node: Langflow ChatOutput node object

        Returns:
            UDF blob data for ChatOutput handling
        """
        # Create a simple ChatOutput UDF that passes through its input
        chat_output_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import HandleInput, Output
from langflow.schema.message import Message

class ChatOutputComponent(Component):
    display_name = "Chat Output"
    description = "Processes chat output for workflow"

    inputs = [
        HandleInput(
            name="input_message",
            display_name="Input Message",
            input_types=["Message", "str"],
        )
    ]

    outputs = [
        Output(display_name="Output", name="output", method="process_output")
    ]

    def process_output(self) -> Message:
        """Process the output message."""
        input_msg = self.input_message

        # Handle different input types
        if hasattr(input_msg, 'text'):
            # It's already a Message object
            return input_msg
        elif isinstance(input_msg, dict):
            # It's a dict with message fields
            return Message(
                text=input_msg.get('text', str(input_msg)),
                sender=input_msg.get('sender', 'AI'),
                sender_name=input_msg.get('sender_name', 'Assistant')
            )
        else:
            # Convert to string and create Message
            return Message(
                text=str(input_msg),
                sender="AI",
                sender_name="Assistant"
            )
'''

        return {
            "code": chat_output_code,
            "component_type": "ChatOutputComponent",
            "template": {
                "input_message": {
                    "type": "Message",
                    "value": None,
                    "info": "Input message to output",
                }
            },
            "outputs": [
                {"name": "output", "method": "process_output", "types": ["Message"]}
            ],
            "selected_output": "output",
        }

    def _create_file_component_udf(self, node: dict[str, Any]) -> dict[str, Any]:
        """Create UDF blob data for File component with mock content.

        Args:
            node: Langflow File node object

        Returns:
            UDF blob data for File component handling with mock content
        """
        file_component_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.message import Message

class MockFileComponent(Component):
    display_name = "Mock File"
    description = "Mock file component that provides sample content for testing"

    outputs = [
        Output(display_name="Raw Content", name="message", method="load_files_message")
    ]

    def load_files_message(self) -> Message:
        """Return mock file content."""
        mock_content = """Sample Document Content

This is a mock document that serves as sample content for testing the document
Q&A workflow.

Key information:
- This document discusses various topics related to AI and machine learning
- It contains technical concepts and explanations
- The content is suitable for question-answering tasks
- You can ask questions about AI, machine learning, or general topics

Technical Details:
- Machine learning is a subset of artificial intelligence
- Deep learning uses neural networks with multiple layers
- Natural language processing helps computers understand human language
- Large language models are trained on vast amounts of text data

This mock content allows testing of the document processing pipeline without
requiring actual file uploads."""

        return Message(
            text=mock_content,
            sender="System",
            sender_name="File Component"
        )
'''

        return {
            "code": file_component_code,
            "component_type": "MockFileComponent",
            "template": {
                "path": {
                    "type": "file",
                    "value": "",
                    "info": "Mock file path (not used in testing)",
                }
            },
            "outputs": [
                {
                    "name": "message",
                    "method": "load_files_message",
                    "types": ["Message"],
                }
            ],
            "selected_output": "message",
        }

    def _create_memory_component_udf(self, node: dict[str, Any]) -> dict[str, Any]:
        """Create UDF blob data for Memory component with mock memory.

        Args:
            node: Langflow Memory node object

        Returns:
            UDF blob data for Memory component handling with mock memory
        """
        memory_component_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.message import Message
from typing import List

class MockMemoryComponent(Component):
    display_name = "Mock Memory"
    description = "Mock memory component that provides sample conversation history"

    outputs = [
        Output(
            display_name="Messages Text",
            name="messages_text",
            method="get_messages_text"
        )
    ]

    def get_messages_text(self) -> str:
        """Return mock conversation history."""
        mock_history = """Previous conversation history:

User: Hello, how are you?
Assistant: Hello! I'm doing well, thank you for asking. I'm here to help with any
questions or tasks you have.

User: What can you help me with?
Assistant: I can help with a wide variety of tasks including answering questions,
writing content, analysis, problem-solving, and general conversation. What would you
like assistance with today?

User: Tell me about artificial intelligence.
Assistant: Artificial intelligence (AI) refers to computer systems that can perform
tasks that typically require human intelligence, such as learning, reasoning, and
problem-solving. AI encompasses various techniques like machine learning, deep
learning, and natural language processing.
"""
        return mock_history.strip()
'''

        return {
            "code": memory_component_code,
            "component_type": "MockMemoryComponent",
            "template": {
                "messages": {
                    "type": "list",
                    "value": [],
                    "info": "Mock message history (not used in testing)",
                }
            },
            "outputs": [
                {
                    "name": "messages_text",
                    "method": "get_messages_text",
                    "types": ["str"],
                }
            ],
            "selected_output": "messages_text",
        }

    def _create_url_component_udf(self, node: dict[str, Any]) -> dict[str, Any]:
        """Create UDF blob data for URL component with mock web content.

        Args:
            node: Langflow URL node object

        Returns:
            UDF blob data for URL component handling with mock web content
        """
        url_component_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.data import Data
from typing import List

class MockURLComponent(Component):
    display_name = "Mock URL"
    description = "Mock URL component that provides sample web content"

    outputs = [
        Output(display_name="Data", name="data", method="fetch_content")
    ]

    def fetch_content(self) -> List[Data]:
        """Return mock web content as Data objects."""
        mock_content_1 = """Mock Web Page 1: Mathematics Calculator

This is a mock web page about mathematical calculations and tools.

Content:
- Online calculator for basic arithmetic operations
- Addition, subtraction, multiplication, and division
- Scientific calculator functions
- Step-by-step solution explanations
- Educational math resources

Example: 2 + 2 = 4
The sum of 2 and 2 equals 4. This is a fundamental arithmetic operation."""

        mock_content_2 = """Mock Web Page 2: Mathematical Tools and Resources

This page contains various mathematical tools and educational content.

Features:
- Interactive problem solver
- Math formula reference
- Practice problems with solutions
- Tutorial videos and explanations
- Advanced mathematical concepts

For simple addition like 2 + 2, the answer is always 4."""

        # Create Data objects with the mock content
        data_objects = [
            Data(
                text=mock_content_1,
                data={
                    "url": "https://example.com/calculator",
                    "title": "Mock Calculator Page",
                    "content_type": "text/html"
                }
            ),
            Data(
                text=mock_content_2,
                data={
                    "url": "https://example.com/math-tools",
                    "title": "Mock Math Tools Page",
                    "content_type": "text/html"
                }
            )
        ]

        return data_objects
'''

        return {
            "code": url_component_code,
            "component_type": "MockURLComponent",
            "template": {
                "url": {
                    "type": "str",
                    "value": "",
                    "info": "Mock URL (not used in testing)",
                }
            },
            "outputs": [{"name": "data", "method": "fetch_content", "types": ["Data"]}],
            "selected_output": "data",
        }

    def _create_simple_mock_component_unused(
        self, component_name: str, output_type: str
    ) -> dict[str, Any]:
        """Create a simple mock component that returns appropriate mock data.

        Args:
            component_name: Name of the component (File, Memory, URLComponent, etc.)
            output_type: Expected output type (Message, str, Data, etc.)

        Returns:
            UDF blob data for the mock component
        """
        # Generate appropriate mock content based on component type
        if component_name == "File":
            mock_content = """Sample Document Content

This is a mock document for testing. It contains information about:
- AI and machine learning concepts
- Technical documentation
- Sample data for Q&A workflows

This mock file allows testing without requiring actual file uploads."""

            code_template = (
                '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.message import Message

class MockFileComponent(Component):
    display_name = "Mock File"
    description = "Mock file component for testing"

    outputs = [Output(display_name="Content", name="message", method="get_content")]

    def get_content(self) -> Message:
        return Message(text="""'''
                + mock_content
                + '''""", sender="System", sender_name="File Component")
'''
            )
            output_info = {
                "name": "message",
                "method": "get_content",
                "types": ["Message"],
            }

        elif component_name == "Memory":
            mock_content = """Previous conversation:
User: Hello, how are you?
Assistant: I'm doing well, thank you! How can I help you today?
User: What did we discuss before?
Assistant: We've had a brief greeting exchange where you asked how I was doing."""

            code_template = (
                '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output

class MockMemoryComponent(Component):
    display_name = "Mock Memory"
    description = "Mock memory component for testing"

    outputs = [
        Output(
            display_name="Messages Text",
            name="messages_text",
            method="get_content"
        )
    ]

    def get_content(self) -> str:
        return """'''
                + mock_content
                + '''"""
'''
            )
            output_info = {
                "name": "messages_text",
                "method": "get_content",
                "types": ["str"],
            }

        elif component_name == "URLComponent":
            mock_content = """Mock Web Content: Mathematics and Calculation Tools

This is sample web content about mathematical calculations:
- Basic arithmetic operations
- Online calculators and tools
- Educational math resources
- Problem-solving guides

Example: 2 + 2 = 4 (basic addition)"""

            code_template = (
                '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output

class MockURLComponentComponent(Component):
    display_name = "Mock URL"
    description = "Mock URL component for testing"

    outputs = [Output(display_name="Content", name="content", method="get_content")]

    def get_content(self) -> str:
        return """'''
                + mock_content
                + '''"""
'''
            )
            output_info = {"name": "content", "method": "get_content", "types": ["str"]}

        else:
            # Generic mock component
            code_template = """
from langflow.custom.custom_component.component import Component
from langflow.io import Output

class MockGenericComponent(Component):
    display_name = "Mock Generic"
    description = "Mock generic component for testing"

    outputs = [Output(display_name="Result", name="result", method="get_content")]

    def get_content(self):
        return "Mock generic result for testing"
"""
            output_info = {"name": "result", "method": "get_content", "types": ["str"]}

        return {
            "code": code_template.strip(),
            "component_type": f"Mock{component_name}Component",
            "template": {},
            "outputs": [output_info],
            "selected_output": output_info["name"],
        }

    def _is_agent_with_tools(
        self,
        node: dict[str, Any],
        dependencies: list[str],
        all_nodes: list[dict[str, Any]],
    ) -> bool:
        """Check if this node is an Agent component with tool dependencies.

        Args:
            node: Langflow node to check
            dependencies: List of dependency node IDs
            all_nodes: All nodes in the workflow

        Returns:
            True if this is an Agent component with tool dependencies
        """
        node_data = node.get("data", {})
        component_type = node_data.get("type", "")

        # Check if this is an Agent component
        if component_type != "Agent":
            return False

        # Check if it has tool dependencies (components that output "Tool" type)
        if not dependencies:
            return False

        # Look for dependencies that create tools
        tool_dependencies = []
        for dep_id in dependencies:
            dep_node = self._find_node_by_id(dep_id, all_nodes)
            if dep_node:
                dep_type = dep_node.get("data", {}).get("type", "")
                # Check if this dependency creates tools
                # (CalculatorComponent, URLComponent, etc.)
                if self._is_tool_creator(dep_type):
                    tool_dependencies.append(dep_id)

        # If agent has tool-creating dependencies, use enhanced UDF machinery
        return len(tool_dependencies) > 0

    def _is_tool_creator(self, component_type: str) -> bool:
        """Check if a component type creates tools.

        Args:
            component_type: Component type to check

        Returns:
            True if this component creates tools
        """
        tool_creators = [
            "CalculatorComponent",
            "URLComponent",
            "SearchAPIComponent",
            "WikipediaAPIComponent",
            "ShellComponent",
            "PythonCodeStructuredTool",
            # Add more tool-creating components as needed
        ]
        return component_type in tool_creators

    def _find_node_by_id(
        self, node_id: str, all_nodes: list[dict[str, Any]]
    ) -> dict[str, Any] | None:
        """Find a node by its ID.

        Args:
            node_id: Node ID to find
            all_nodes: All nodes to search

        Returns:
            Node dictionary if found, None otherwise
        """
        for node in all_nodes:
            if node.get("id") == node_id:
                return node
        return None

    def _create_tool_sequence_config(
        self,
        agent_node: dict[str, Any],
        dependencies: list[str],
        all_nodes: list[dict[str, Any]],
        builder: Any,
        node_output_refs: dict[str, Any],
        field_mapping: dict[str, dict[str, str]] = None,
    ) -> dict[str, Any]:
        """Create tool sequence configuration for enhanced UDF machinery.

        Args:
            agent_node: Agent node that needs tools
            dependencies: Agent dependency node IDs
            all_nodes: All nodes in workflow
            builder: FlowBuilder instance
            node_output_refs: Node output references
            field_mapping: Field mapping for inputs

        Returns:
            Tool sequence configuration for UDF executor
        """
        component_type = agent_node.get("data", {}).get("type", "")

        # Separate tool dependencies from other dependencies
        tool_configs: list[dict[str, Any]] = []
        other_inputs: dict[str, Any] = {}
        external_inputs: dict[str, Any] = {}  # For references to non-fused steps

        for dep_id in dependencies:
            dep_node = self._find_node_by_id(dep_id, all_nodes)
            if dep_node:
                dep_type = dep_node.get("data", {}).get("type", "")

                if self._is_tool_creator(dep_type):
                    # This is a tool dependency - create tool config

                    # Create blob for tool component
                    tool_blob_data = self._prepare_udf_blob(dep_node, dep_type)
                    tool_blob_step_id = f"langflow_tool_blob_{dep_id}"
                    tool_blob_step = builder.add_step(
                        id=tool_blob_step_id,
                        component="/builtin/put_blob",
                        input_data={"data": tool_blob_data, "blob_type": "data"},
                    )

                    # Use intelligent translation approach:
                    # Since blob_id references a step that will be executed
                    # BEFORE this fused step,
                    # we can reference it as an external input to the fused step
                    tool_config = {
                        "component_type": dep_type,
                        "blob_id": f"tool_blob_{len(tool_configs)}",
                        # Internal reference within fused step
                        "inputs": self._extract_tool_inputs(
                            dep_node, all_nodes, node_output_refs
                        ),
                    }
                    tool_configs.append(tool_config)

                    # Add external input mapping for the blob_id
                    external_inputs[f"tool_blob_{len(tool_configs) - 1}"] = Value.step(
                        tool_blob_step.id, "blob_id"
                    )
                else:
                    # This is a regular input dependency (like ChatInput)
                    if dep_type == "ChatInput":
                        other_inputs["input_value"] = Value.input.add_path("message")
                        # Also extract session_id if available in the workflow input
                        other_inputs["session_id_from_input"] = Value.input.add_path(
                            "session_id"
                        )
                    else:
                        dep_step_id = self._generate_step_id(dep_id, dep_type)
                        other_inputs["input_value"] = Value.step(dep_step_id, "result")

        # Extract Agent-specific parameters from the node template
        if component_type == "Agent":
            template = agent_node.get("data", {}).get("node", {}).get("template", {})

            # Add essential Agent parameters with default values
            # Use session_id from ChatInput dependency if available, otherwise fallback
            session_id_value = "default_session"
            if "session_id_from_input" in other_inputs:
                # If we have a ChatInput dependency with session_id, use it
                session_id_value = other_inputs["session_id_from_input"]

            agent_params = {
                "session_id": session_id_value,
                "api_key": "",  # Will be populated by runtime environment
                "model_name": template.get("model_name", {}).get(
                    "value", "gpt-3.5-turbo"
                ),
                "temperature": template.get("temperature", {}).get("value", 0.7),
                "max_tokens": template.get("max_tokens", {}).get("value", 1000),
                "system_prompt": template.get("system_prompt", {}).get(
                    "value", "You are a helpful assistant."
                ),
                "verbose": template.get("verbose", {}).get("value", False),
            }

            # Remove temporary session_id_from_input key before merging
            other_inputs.pop("session_id_from_input", None)

            # Merge agent parameters into other_inputs
            other_inputs.update(agent_params)

        # Create blob for agent component
        agent_blob_data = self._prepare_udf_blob(agent_node, component_type)
        agent_blob_step_id = f"langflow_agent_blob_{agent_node['id']}"
        agent_blob_step = builder.add_step(
            id=agent_blob_step_id,
            component="/builtin/put_blob",
            input_data={"data": agent_blob_data, "blob_type": "data"},
        )

        # Apply intelligent translation: agent blob is also external to the fused step
        external_inputs["agent_blob"] = Value.step(agent_blob_step.id, "blob_id")

        # Create tool sequence configuration with resolved references
        tool_sequence_config = {
            "tools": tool_configs,
            "agent": {
                "component_type": component_type,
                "blob_id": "agent_blob",  # Internal reference within fused step
                "inputs": other_inputs,  # Input_value and other agent inputs
            },
        }

        # Merge external inputs with other inputs
        final_inputs = other_inputs.copy()
        final_inputs.update(external_inputs)

        # Return external inputs at top level for proper step input mapping
        result = {
            "tool_sequence_config": tool_sequence_config,
            "input": other_inputs,  # Only non-external inputs go under "input"
        }

        # Add external inputs at top level so they become step input parameters
        result.update(external_inputs)

        return result

    def _extract_tool_inputs(
        self,
        tool_node: dict[str, Any],
        all_nodes: list[dict[str, Any]],
        node_output_refs: dict[str, Any],
    ) -> dict[str, Any]:
        """Extract inputs for a tool component.

        Args:
            tool_node: Tool node to extract inputs from
            all_nodes: All nodes in workflow
            node_output_refs: Node output references

        Returns:
            Tool component inputs
        """
        node_data = tool_node.get("data", {})
        component_type = node_data.get("type", "")
        template = node_data.get("node", {}).get("template", {})

        tool_inputs: dict[str, Any] = {}

        # Extract all template values
        for field_name, field_config in template.items():
            if isinstance(field_config, dict):
                field_value = field_config.get("value")
                if field_value is not None:
                    tool_inputs[field_name] = field_value

        # Apply component-specific default values for empty/missing required fields
        if component_type == "URLComponent":
            urls_value = tool_inputs.get("urls", "")
            if not urls_value or urls_value.strip() == "":
                # Provide default URL for tool creation - agents can override at runtime
                # Use httpbin.org which is designed for testing and responds quickly
                tool_inputs["urls"] = ["https://httpbin.org/html"]

        return tool_inputs

    def _is_tool_dependency_of_agent(
        self,
        node_id: str,
        all_nodes: list[dict[str, Any]],
        dependencies: dict[str, list[str]],
    ) -> bool:
        """Check if a node is a tool dependency of an Agent node.

        Args:
            node_id: ID of the node to check
            all_nodes: All nodes in the workflow
            dependencies: Dependency graph

        Returns:
            True if this node is a tool dependency of an Agent
        """
        # Look for any Agent nodes that have this node as a dependency
        for node in all_nodes:
            agent_node_id = node.get("id")
            if not agent_node_id:
                continue

            # Check if this node is an Agent with tools
            if self._is_agent_with_tools(
                node, dependencies.get(agent_node_id, []), all_nodes
            ):
                # Check if the current node_id is a dependency of this Agent
                agent_dependencies = dependencies.get(agent_node_id, [])
                if node_id in agent_dependencies:
                    # Verify it's a tool dependency (not a regular input like ChatInput)
                    dep_node = self._find_node_by_id(node_id, all_nodes)
                    if dep_node:
                        dep_type = dep_node.get("data", {}).get("type", "")
                        if self._is_tool_creator(dep_type):
                            return True

        return False
