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
        output_mapping: dict[str, str] = None,
        mode_classification: dict[str, str] = None,
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
            output_mapping: Mapping of source node IDs to their selected output
                names from edges
            mode_classification: Mapping of node IDs to mode classification
                ("ingest" or "retrieve") for skip conditions

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

            # Check if this component should be treated as a tool
            node_info = node_data.get("node", {})
            template = node_info.get("template", {})

            # Component is a tool if it has tool_mode=True at the component level
            is_tool_component = node_info.get("tool_mode", False)

            if is_tool_component:
                return self._create_tool_component_step(
                    node,
                    step_id,
                    builder,
                    dependencies,
                    node_output_refs,
                    field_mapping or {},
                    output_mapping or {},
                )

            # For regular components, create a UDF step
            custom_code = template.get("code", {}).get("value", "")

            # Determine component path and inputs based on available information
            if custom_code:
                # Any component with code - use UDF executor for real execution

                # Routing to UDF executor
                component_path = "/langflow/udf_executor"

                # First create a blob step for the UDF code using auto ID generation
                blob_data = self._prepare_udf_blob(node, component_type, output_mapping)

                blob_step_id = f"{step_id}_blob"
                blob_step_handle = builder.add_step(
                    id=blob_step_id,
                    component="/builtin/put_blob",
                    input_data={"data": blob_data, "blob_type": "data"},
                    must_execute=True,
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
            else:
                # All executable components should have custom code
                # If we reach here, the fixture may be incomplete
                raise ConversionError(
                    f"Component {component_type} in node {node_id} has no custom code. "
                    f"All executable components should have custom code."
                )

            # Add step to builder with proper ID and component path
            step_id = self._generate_step_id(node_id, component_type)

            # Add metadata to mark vector store components
            step_metadata = {}
            if "VectorStore" in blob_data.get("base_classes", []):
                step_metadata["is_vector_store"] = True
                step_metadata["vector_store_outputs"] = blob_data.get("outputs", [])

            # Add skip condition based on mode classification
            skip_if = self._create_mode_skip_condition(
                node_id, mode_classification or {}
            )

            step_handle = builder.add_step(
                id=step_id,
                component=component_path,
                input_data=step_input,
                must_execute=True,
                skip_if=skip_if,
                metadata=step_metadata if step_metadata else None,
            )

            # Return a reference to this step's output
            return Value.step(step_handle.id, "result")

        except Exception as e:
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
        # Preserve original case for case-sensitive comparisons
        base_id = node_id

        # Always use langflow prefix with the full base_id to guarantee uniqueness
        return f"langflow_{base_id}"

    def _prepare_udf_blob(
        self,
        node: dict[str, Any],
        component_type: str,
        output_mapping: dict[str, str] = None,
    ) -> dict[str, Any]:
        """Prepare enhanced UDF blob data for component execution.

        Args:
            node: Langflow node object
            component_type: Component type name
            output_mapping: Mapping of node IDs to their selected output names

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

        # Determine selected output - use output mapping from edges if available
        selected_output = None
        node_id = node.get("id")
        if output_mapping and node_id in output_mapping:
            # Use the output specified in the edge
            selected_output = output_mapping[node_id]
        elif final_outputs:
            # Fallback to the first output if no edge mapping found
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
        for field_name, field_config in template.items():
            if field_name != "code":
                prepared_template[field_name] = field_config

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

        # Enhanced blob created with component metadata

        return blob_data

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

        # Special case: Agent components need session_id even if not in template
        # Agent uses self.graph.session_id for memory retrieval
        if component_type == "Agent":
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

    def _create_tool_component_step(
        self,
        node: dict[str, Any],
        step_id: str,
        builder: FlowBuilder,
        dependencies: dict[str, list[str]],
        node_output_refs: dict[str, Any],
        field_mapping: dict[str, dict[str, str]],
        output_mapping: dict[str, str],
    ) -> Any:
        """Create a component_tool step for tool-mode components.

        Args:
            node: Langflow node object
            step_id: Generated step ID
            builder: FlowBuilder instance
            dependencies: Dependency graph
            node_output_refs: Node output references
            field_mapping: Field mapping from edges

        Returns:
            Output reference for the tool wrapper step
        """
        try:
            node_id = node.get("id", "")
            node_data = node.get("data", {})
            component_type = node_data.get("type", "")
            node_info = node_data.get("node", {})

            # Extract component inputs from dependencies and field mapping
            component_inputs = self._build_component_inputs(
                node_id, dependencies, node_output_refs, field_mapping
            )

            # Create step that calls component_tool to create tool wrapper
            step_handle = builder.add_step(
                id=step_id,
                component="/langflow/component_tool",
                input_data={
                    "code": Value.literal(
                        node_info
                    ),  # Store entire component definition
                    "inputs": component_inputs,  # Static inputs from workflow
                    "component_type": component_type,
                },
                must_execute=True,
            )

            # Return a reference to this step's output (the tool wrapper)
            # The component_tool returns the tool wrapper under "result" field
            return Value.step(step_handle.id, "result")

        except Exception as e:
            raise ConversionError(f"Error creating tool component step: {e}") from e

    def _build_component_inputs(
        self,
        node_id: str,
        dependencies: dict[str, list[str]],
        node_output_refs: dict[str, Any],
        field_mapping: dict[str, dict[str, str]],
    ) -> dict[str, Any]:
        """Build input dict for a component from its dependencies.

        Args:
            node_id: Target node ID
            dependencies: Dependency graph
            node_output_refs: Output references from other nodes
            field_mapping: Field name mapping from edges

        Returns:
            Dict mapping input field names to their values/references
        """
        inputs = {}

        # Get dependencies for this node
        deps = dependencies.get(node_id, [])
        field_map = field_mapping.get(node_id, {})

        for dep_node_id in deps:
            if dep_node_id in node_output_refs:
                # Get field name this dependency maps to
                field_name = field_map.get(dep_node_id, "input")
                # Map dependency output to input field
                inputs[field_name] = node_output_refs[dep_node_id]

        return inputs

    def _create_mode_skip_condition(
        self, node_id: str, mode_classification: dict[str, str]
    ) -> Value | None:
        """Create skip condition based on mode classification.

        Args:
            node_id: Node ID to check classification for
            mode_classification: Dict mapping node IDs to mode classification

        Returns:
            Value representing skip condition, or None if no skip needed
        """
        classification = mode_classification.get(node_id)
        if not classification:
            return None

        # Reference the appropriate skip flag from mode_check step
        # mode_check returns skip_ingest_step and skip_retrieve_step fields
        # which are True when the step should be skipped
        if classification == "ingest":
            return Value.step("mode_check", "skip_ingest_step")
        else:  # retrieve
            return Value.step("mode_check", "skip_retrieve_step")
