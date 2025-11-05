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

"""Main Langflow to Stepflow converter implementation."""

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import msgspec
import yaml
from stepflow_py import Flow, FlowBuilder, Step, Value
from stepflow_py.generated_flow import OnSkipDefault

from ..exceptions import ConversionError
from .dependency_analyzer import DependencyAnalyzer
from .node_processor import NodeProcessor
from .schema_mapper import SchemaMapper


@dataclass
class WorkflowAnalysis:
    """Typed analysis results from analyzing a Langflow workflow."""

    node_count: int  # Total number of nodes in the Langflow workflow
    edge_count: int  # Total number of connections/edges between nodes
    # Map of component type names to their counts (e.g., {"ChatInput": 1, "OpenAI": 2})
    component_types: dict[str, int]
    dependencies: dict[
        str, list[str]
    ]  # Map of node IDs to lists of their dependency node IDs
    potential_issues: list[
        str
    ]  # List of warnings or potential problems detected during analysis


class LangflowConverter:
    """Convert Langflow JSON workflows to Stepflow YAML workflows."""

    def __init__(self):
        """Initialize the converter."""
        self.dependency_analyzer = DependencyAnalyzer()
        self.schema_mapper = SchemaMapper()
        self.node_processor = NodeProcessor()

    def convert_file(self, input_path: str | Path) -> str:
        """Convert a Langflow JSON file to Stepflow YAML.

        Args:
            input_path: Path to the Langflow JSON file

        Returns:
            Stepflow YAML as a string

        Raises:
            ConversionError: If conversion fails
            ValidationError: If validation fails
        """
        input_path = Path(input_path)
        if not input_path.exists():
            raise ConversionError(f"Input file not found: {input_path}")

        try:
            with open(input_path, encoding="utf-8") as f:
                langflow_data = json.load(f)
        except json.JSONDecodeError as e:
            raise ConversionError(f"Invalid JSON in {input_path}: {e}") from e
        except Exception as e:
            raise ConversionError(f"Error reading {input_path}: {e}") from e

        workflow = self.convert(langflow_data)
        return self.to_yaml(workflow)

    def convert(self, langflow_data: dict[str, Any]) -> Flow:
        """Convert Langflow data structure to Stepflow workflow.

        Args:
            langflow_data: Parsed Langflow JSON data

        Returns:
            Flow object

        Raises:
            ConversionError: If conversion fails
        """
        try:
            # Extract main data structure
            if "data" not in langflow_data:
                raise ConversionError("Invalid Langflow JSON: missing 'data' key")

            data = langflow_data["data"]
            nodes = data.get("nodes", [])
            edges = data.get("edges", [])

            if not nodes:
                raise ConversionError("No nodes found in Langflow workflow")

            # Build dependency graph from edges
            dependencies = self.dependency_analyzer.build_dependency_graph(edges)

            # Get proper execution order using topological sort
            execution_order = self.dependency_analyzer.get_execution_order(dependencies)

            # Create field mapping from edges for proper UDF input handling
            field_mapping = self._build_field_mapping_from_edges(edges)

            # Create output mapping from edges to track which output is used from
            # each component
            output_mapping = self._build_output_mapping_from_edges(edges)

            # Create node lookup for efficient processing
            node_lookup = {node["id"]: node for node in nodes}

            # Create FlowBuilder
            builder = FlowBuilder(name=self._generate_workflow_name(langflow_data))

            # Detect vector store components in the workflow
            has_vector_stores = self._has_vector_store_components(nodes)

            # Add mode parameter input schema and classify nodes if vector
            # stores present
            node_mode_classification: dict[str, str] = {}
            if has_vector_stores:
                # Schema is defined as an arbitrary JSON object
                # (additionalProperties: true in JSON schema)
                # We store it as a dict and it will be serialized properly
                input_schema = {
                    "type": "object",
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["ingest", "retrieve", "hybrid"],
                            "default": "hybrid",
                            "description": (
                                "Execution mode for vector store operations. "
                                "'ingest': populate vector stores with documents. "
                                "'retrieve': search vector stores with queries. "
                                "'hybrid': automatically ingest if documents provided, "
                                "then retrieve if query provided."
                            ),
                        },
                    },
                }
                # Set directly on builder bypassing set_input_schema method
                # Type ignore needed because Schema is a union type
                builder.input_schema = input_schema  # type: ignore[assignment]

                # Classify nodes for mode-aware skip conditions
                node_mode_classification = self._classify_nodes_for_mode_skipping(
                    nodes, dependencies
                )

                # Add mode_check step at the beginning
                # Provide default value "hybrid" for mode to make it optional
                mode_on_skip = OnSkipDefault(action="useDefault", defaultValue="hybrid")
                mode_check_handle = builder.add_step(
                    id="mode_check",
                    component="/langflow/mode_check",
                    input_data={"mode": Value.input("mode", on_skip=mode_on_skip)},
                )

            # Process nodes and collect output references
            node_output_refs: dict[str, Any] = {}  # node_id -> output reference
            processed_nodes = set()

            # Store mode_check handle for skip condition references
            if has_vector_stores:
                node_output_refs["__mode_check__"] = mode_check_handle

            # First, process nodes in execution order
            for node_id in execution_order:
                if node_id in node_lookup:
                    output_ref = self.node_processor.process_node(
                        node_lookup[node_id],
                        dependencies,
                        nodes,
                        builder,
                        node_output_refs,
                        field_mapping,
                        output_mapping,
                        node_mode_classification,
                    )
                    if output_ref is not None:
                        node_output_refs[node_id] = output_ref
                    processed_nodes.add(node_id)

            # Then, process any remaining nodes (nodes with no dependencies)
            for node in nodes:
                node_id = node["id"]
                if node_id not in processed_nodes:
                    output_ref = self.node_processor.process_node(
                        node,
                        dependencies,
                        nodes,
                        builder,
                        node_output_refs,
                        field_mapping,
                        output_mapping,
                        node_mode_classification,
                    )
                    if output_ref is not None:
                        node_output_refs[node_id] = output_ref

            # For vector store workflows, add a mode_output step
            if has_vector_stores:
                # Find the ChatOutput node to get its dependency (the retrieval result)
                chat_output_nodes = [
                    n for n in nodes if n.get("data", {}).get("type") == "ChatOutput"
                ]
                retrieval_result_input = {}
                if chat_output_nodes:
                    chat_output_node = chat_output_nodes[0]
                    chat_output_id = chat_output_node["id"]
                    if chat_output_id in dependencies and dependencies[chat_output_id]:
                        dep_node_id = dependencies[chat_output_id][0]
                        if dep_node_id in node_output_refs:
                            # Get the step ID for the retrieval result
                            # The dep_node_id is the Langflow node ID, we need
                            # the stepflow step ID
                            step_id = f"langflow_{dep_node_id}"
                            # Use OnSkipDefault to provide None when the step is skipped
                            on_skip = OnSkipDefault(
                                action="useDefault", defaultValue=None
                            )
                            retrieval_result_input["retrieval_result"] = Value.step(
                                step_id, "result", on_skip=on_skip
                            )

                # Add mode_output step - must always execute to produce workflow output
                # Use same default for mode as mode_check
                mode_output_on_skip = OnSkipDefault(
                    action="useDefault", defaultValue="hybrid"
                )
                mode_output_handle = builder.add_step(
                    id="mode_output",
                    component="/langflow/mode_output",
                    input_data={
                        "mode": Value.input("mode", on_skip=mode_output_on_skip),
                        **retrieval_result_input,
                    },
                    must_execute=True,  # Always run to provide output
                )
                node_output_refs["__mode_output__"] = mode_output_handle

            # Set workflow output using incremental output building
            self._build_flow_output(
                builder, nodes, dependencies, node_output_refs, has_vector_stores
            )

            # Build and return the flow
            flow = builder.build()
            return flow

        except ConversionError:
            raise
        except Exception as e:
            raise ConversionError(f"Unexpected error during conversion: {e}") from e

    def to_yaml(self, workflow: Flow) -> str:
        """Convert Flow to YAML string.

        Args:
            workflow: Flow object (official stepflow_py type)

        Returns:
            YAML string
        """
        try:
            # Convert Flow to dict using msgspec serialization
            workflow_dict = msgspec.to_builtins(workflow)

            # Generate clean YAML
            return yaml.dump(
                workflow_dict,
                default_flow_style=False,
                sort_keys=False,
                allow_unicode=True,
                width=120,
            )
        except Exception as e:
            raise ConversionError(f"Error generating YAML: {e}") from e

    def analyze(self, langflow_data: dict[str, Any]) -> WorkflowAnalysis:
        """Analyze Langflow workflow structure without conversion.

        Args:
            langflow_data: Parsed Langflow JSON data

        Returns:
            Typed analysis results
        """
        try:
            data = langflow_data.get("data", {})
            nodes = data.get("nodes", [])
            edges = data.get("edges", [])

            # Basic statistics
            analysis: dict[str, Any] = {
                "node_count": len(nodes),
                "edge_count": len(edges),
                "component_types": {},
                "dependencies": {},
                "potential_issues": [],
            }

            # Analyze nodes
            for node in nodes:
                node_data = node.get("data", {})
                component_type = node_data.get("type", "Unknown")

                if component_type not in analysis["component_types"]:
                    analysis["component_types"][component_type] = 0
                analysis["component_types"][component_type] += 1

                # Check for potential issues
                if not node.get("id"):
                    analysis["potential_issues"].append("Node missing ID")
                if not node_data.get("node", {}).get("template"):
                    analysis["potential_issues"].append(
                        f"Node {node.get('id', 'unknown')} missing template"
                    )

            # Analyze dependencies
            dependencies = self.dependency_analyzer.build_dependency_graph(edges)

            return WorkflowAnalysis(
                node_count=len(nodes),
                edge_count=len(edges),
                component_types=analysis["component_types"],
                dependencies=dependencies,
                potential_issues=analysis["potential_issues"],
            )

        except Exception as e:
            raise ConversionError(f"Error analyzing workflow: {e}") from e

    def _generate_workflow_name(self, langflow_data: dict[str, Any]) -> str:
        """Generate a workflow name from Langflow data."""
        # Try to get name from various sources
        if "name" in langflow_data:
            name = langflow_data["name"]
            return str(name) if name is not None else "Converted Langflow Workflow"

        data = langflow_data.get("data", {})
        if "name" in data:
            name = data["name"]
            return str(name) if name is not None else "Converted Langflow Workflow"

        # Fallback to generic name
        return "Converted Langflow Workflow"

    def _has_vector_store_components(self, nodes: list[dict[str, Any]]) -> bool:
        """Check if the workflow contains vector store components.

        Args:
            nodes: List of Langflow nodes

        Returns:
            True if any node is a vector store component
        """
        for node in nodes:
            node_data = node.get("data", {})
            # Check base_classes for VectorStore (in node.data.node.base_classes)
            node_info = node_data.get("node", {})
            base_classes = node_info.get("base_classes", [])
            if "VectorStore" in base_classes:
                return True

            # Also check the type field as fallback
            component_type = node_data.get("type", "")
            if (
                "vectorstore" in component_type.lower()
                or "vector_store" in component_type.lower()
            ):
                return True

        return False

    def _classify_nodes_for_mode_skipping(
        self,
        nodes: list[dict[str, Any]],
        dependencies: dict[str, list[str]],
    ) -> dict[str, str]:
        """Classify nodes for mode-aware skip conditions in vector store workflows.

        Args:
            nodes: List of Langflow nodes
            dependencies: Node dependency graph

        Returns:
            Dict mapping node_id -> mode classification:
            - "ingest": Node is part of ingestion path (skip when mode='retrieve')
            - "retrieve": Node is part of retrieval path (skip when mode='ingest')
            - None: Node runs in all modes (no skip condition)
        """
        classification: dict[str, str] = {}

        # Find vector store nodes
        vector_store_nodes = set()
        for node in nodes:
            node_data = node.get("data", {})
            node_info = node_data.get("node", {})
            base_classes = node_info.get("base_classes", [])
            if "VectorStore" in base_classes:
                vector_store_nodes.add(node["id"])

        if not vector_store_nodes:
            return classification  # No vector stores, no classification needed

        # Build reverse dependency graph (dependents)
        dependents: dict[str, list[str]] = {}
        for node_id, deps in dependencies.items():
            for dep_id in deps:
                if dep_id not in dependents:
                    dependents[dep_id] = []
                dependents[dep_id].append(node_id)

        # Helper to find all ancestors (dependencies) of a node
        def get_ancestors(node_id: str, visited: set | None = None) -> set[str]:
            if visited is None:
                visited = set()
            if node_id in visited:
                return visited
            visited.add(node_id)
            for dep_id in dependencies.get(node_id, []):
                get_ancestors(dep_id, visited)
            return visited

        # Helper to find all descendants (dependents) of a node
        def get_descendants(node_id: str, visited: set | None = None) -> set[str]:
            if visited is None:
                visited = set()
            if node_id in visited:
                return visited
            visited.add(node_id)
            for dep_id in dependents.get(node_id, []):
                get_descendants(dep_id, visited)
            return visited

        # Create nodes lookup for ancestor checking
        nodes_lookup = {n["id"]: n for n in nodes}

        # Classify based on component type and position relative to vector stores
        for node in nodes:
            node_id = node["id"]
            node_data = node.get("data", {})
            component_type = node_data.get("type", "")

            # ChatInput and Prompt components are typically retrieval-focused
            if component_type in ["ChatInput", "Prompt", "LanguageModelComponent"]:
                classification[node_id] = "retrieve"
                continue

            # File and document processing components are ingestion-focused
            if component_type in ["File", "SplitText"]:
                classification[node_id] = "ingest"
                continue

            # For other nodes (including vector stores), classify based on dependencies
            ancestors = get_ancestors(node_id)

            # Check if dependencies include known ingestion or retrieval components
            has_file_ancestor = any(
                nodes_lookup.get(aid, {}).get("data", {}).get("type") == "File"
                for aid in ancestors
            )
            has_chatinput_ancestor = any(
                nodes_lookup.get(aid, {}).get("data", {}).get("type") == "ChatInput"
                for aid in ancestors
            )

            # Parser is special - it processes retrieval results
            if component_type == "parser":
                classification[node_id] = "retrieve"
                continue

            # Classify based on ancestors
            if has_file_ancestor and not has_chatinput_ancestor:
                classification[node_id] = "ingest"
            elif has_chatinput_ancestor and not has_file_ancestor:
                classification[node_id] = "retrieve"
            # If has both or neither, use position-based heuristic
            elif node_id in vector_store_nodes:
                # Vector stores with File ancestors are ingestion
                # Vector stores with ChatInput ancestors are retrieval
                if has_file_ancestor:
                    classification[node_id] = "ingest"
                elif has_chatinput_ancestor:
                    classification[node_id] = "retrieve"

        # Second pass: propagate classification backwards from classified nodes
        # to their unclassified dependencies (e.g., embeddings feeding into
        # vector stores)
        changed = True
        iteration = 0
        while changed:
            changed = False
            iteration += 1
            # Iterate over ALL nodes, not just those in dependencies dict
            for node in nodes:
                node_id = node["id"]
                if node_id in classification:
                    continue  # Already classified

                # Check if all descendants (nodes that depend on this) have the
                # same classification
                node_dependents = dependents.get(node_id, [])
                if not node_dependents:
                    continue

                # Get classifications of dependents
                dependent_classifications = {
                    classification[dep_id]
                    for dep_id in node_dependents
                    if dep_id in classification
                }

                # If all dependents have the same classification, apply it to this node
                if len(dependent_classifications) == 1:
                    new_classification = dependent_classifications.pop()
                    classification[node_id] = new_classification
                    changed = True

        return classification

    def _build_field_mapping_from_edges(
        self, edges: list[dict[str, Any]]
    ) -> dict[str, dict[str, str]]:
        """Build field mapping from edges for proper input handling.

        Args:
            edges: List of Langflow edges

        Returns:
            Dict mapping target_node_id -> {source_node_id -> target_field_name}
        """
        field_mapping: dict[str, dict[str, str]] = {}

        for edge in edges:
            target_id = edge.get("target")
            source_id = edge.get("source")

            if not target_id or not source_id:
                continue

            # Get target field name from edge data
            edge_data = edge.get("data", {})
            target_handle = edge_data.get("targetHandle", {})

            if isinstance(target_handle, dict):
                field_name = target_handle.get("fieldName")
            elif isinstance(target_handle, str):
                # Sometimes targetHandle is a JSON string - handle this case
                try:
                    import json

                    target_info = json.loads(target_handle.replace("œ", '"'))
                    field_name = target_info.get("fieldName")
                except (json.JSONDecodeError, KeyError, TypeError, AttributeError):
                    field_name = None
            else:
                field_name = None

            if field_name:
                if target_id not in field_mapping:
                    field_mapping[target_id] = {}
                field_mapping[target_id][source_id] = field_name

        return field_mapping

    def _build_output_mapping_from_edges(
        self, edges: list[dict[str, Any]]
    ) -> dict[str, str]:
        """Build output mapping from edges to track output usage per component.

        Args:
            edges: List of Langflow edges

        Returns:
            Dict mapping source_node_id -> output_name
        """
        output_mapping: dict[str, str] = {}

        for edge in edges:
            source_id = edge.get("source")

            if not source_id:
                continue

            # Get source output name from edge data
            edge_data = edge.get("data", {})
            source_handle = edge_data.get("sourceHandle", {})

            output_name = None
            if isinstance(source_handle, dict):
                output_name = source_handle.get("name")
            elif isinstance(source_handle, str):
                # Sometimes sourceHandle is a JSON string - handle this case
                try:
                    import json

                    source_info = json.loads(source_handle.replace("œ", '"'))
                    output_name = source_info.get("name")
                except (json.JSONDecodeError, KeyError, TypeError, AttributeError):
                    output_name = None

            # Only store if we found an output name and don't have one already
            # (first edge wins if component has multiple outgoing edges with
            # different outputs)
            if output_name and source_id not in output_mapping:
                output_mapping[source_id] = output_name

        return output_mapping

    def _generate_input_section(
        self, nodes: list[dict[str, Any]]
    ) -> dict[str, Any] | None:
        """Generate input section for workflow based on ChatInput components.

        Args:
            nodes: List of Langflow nodes

        Returns:
            Input section dict or None if no ChatInput components found
        """
        # Look for ChatInput components
        chat_input_nodes = []
        for node in nodes:
            node_data = node.get("data", {})
            component_type = node_data.get("type", "")
            if component_type == "ChatInput":
                chat_input_nodes.append(node)

        if not chat_input_nodes:
            return None

        # Generate input schema for ChatInput components
        # For now, assume all ChatInput components expect a "message" field
        input_schema = {
            "message": {
                "type": "string",
                "description": "Message input for the chat workflow",
            }
        }

        # If multiple ChatInput components, we might need more complex input schema
        if len(chat_input_nodes) > 1:
            # For multiple inputs, create named fields based on node IDs
            input_schema = {}
            for node in chat_input_nodes:
                node_id = node.get("id", "")
                field_name = f"message_{node_id.lower().replace('-', '_')}"
                input_schema[field_name] = {
                    "type": "string",
                    "description": f"Message input for {node_id}",
                }

        return input_schema

    def _generate_output_section(
        self,
        steps: list[Step],
        dependencies: dict[str, list[str]],
        nodes: list[dict[str, Any]] = None,
    ) -> dict[str, Any] | None:
        """Generate output section for workflow based on step types and dependencies.

        Args:
            steps: List of workflow steps
            dependencies: Dependency graph
            nodes: Original Langflow nodes (for handling no-step workflows)

        Returns:
            Output section dict or None if no obvious output step
        """
        if not steps:
            # Handle workflows with no steps (ChatInput → ChatOutput)
            if nodes:
                chat_input_nodes = [
                    n for n in nodes if n.get("data", {}).get("type") == "ChatInput"
                ]
                chat_output_nodes = [
                    n for n in nodes if n.get("data", {}).get("type") == "ChatOutput"
                ]

                if chat_input_nodes and chat_output_nodes:
                    # Direct passthrough from input to output
                    return {"$from": {"workflow": "input"}, "path": "message"}
            return None

        # Find output steps (steps with no dependents or known output types)
        output_steps = []

        # Find steps that nothing else depends on (leaf nodes)
        dependent_steps = set()
        for deps in dependencies.values():
            dependent_steps.update(deps)

        leaf_steps = [step for step in steps if step.id not in dependent_steps]

        # First, look for ChatOutput components specifically (highest priority)
        for step in steps:
            component_lower = step.component.lower()
            step_id_lower = step.id.lower()
            # Check if this is a ChatOutput component (either direct or UDF)
            if "chatoutput" in step_id_lower or "chat_output" in step_id_lower:
                output_steps.append(step)

        # If no ChatOutput found, look for other output component types
        if not output_steps:
            for step in leaf_steps:
                component_lower = step.component.lower()
                if any(output_type in component_lower for output_type in ["output"]):
                    output_steps.append(step)
                # Also prioritize steps that were originally output components
                # (now using identity)
                elif step.component == "/builtin/identity":
                    output_steps.append(step)

        # If no specific output components, use all leaf steps
        if not output_steps:
            output_steps = leaf_steps

        # If still no output steps, use the last step
        if not output_steps and steps:
            output_steps = [steps[-1]]

        # Generate output section
        if len(output_steps) == 1:
            # Single output step - return its result directly
            step = output_steps[0]
            return {"$from": {"step": step.id}, "path": "result"}
        elif len(output_steps) > 1:
            # Multiple output steps - create a structured result
            result = {}
            for step in output_steps:
                # Use a cleaned version of step ID as the key
                key = step.id.replace("-", "_").lower()
                if "output" in key:
                    key = "result"  # Simplify output step names
                elif "chat" in key:
                    key = "message"

                result[key] = {"$from": {"step": step.id}, "path": "result"}
            return result

        return None

    def _build_flow_output(
        self,
        builder: FlowBuilder,
        nodes: list[dict[str, Any]],
        dependencies: dict[str, list[str]],
        node_output_refs: dict[str, Any],
        has_vector_stores: bool = False,
    ) -> None:
        """Build workflow output using incremental output building API.

        Args:
            builder: FlowBuilder instance to add output fields to
            nodes: Original Langflow nodes
            dependencies: Dependency graph
            node_output_refs: Mapping of node IDs to their output references
            has_vector_stores: Whether this workflow contains vector stores
        """
        # For vector store workflows, use the mode_output step
        if has_vector_stores and "__mode_output__" in node_output_refs:
            builder.set_output(Value.step("mode_output", "message"))
            return

        # Look for ChatOutput nodes first
        chat_output_nodes = [
            n for n in nodes if n.get("data", {}).get("type") == "ChatOutput"
        ]

        if chat_output_nodes:
            # Use the first ChatOutput node
            chat_output_node = chat_output_nodes[0]
            node_id = chat_output_node["id"]

            # Check if ChatOutput has dependencies
            if node_id in dependencies and dependencies[node_id]:
                # ChatOutput depends on another node - use that node's output
                dep_node_id = dependencies[node_id][0]
                if dep_node_id in node_output_refs:
                    builder.set_output(node_output_refs[dep_node_id])
                    return

            # ChatOutput has no dependencies or dependencies not found - check if
            # it's a simple passthrough
            if chat_output_nodes and len(nodes) <= 2:
                # Simple ChatInput -> ChatOutput workflow
                chat_input_nodes = [
                    n for n in nodes if n.get("data", {}).get("type") == "ChatInput"
                ]
                if chat_input_nodes:
                    builder.set_output(Value.input.add_path("message"))
                    return

        # Find leaf nodes (nodes with no dependents)
        dependent_nodes = set()
        for deps in dependencies.values():
            dependent_nodes.update(deps)

        leaf_nodes = []
        for node in nodes:
            node_id = node["id"]
            component_type = node.get("data", {}).get("type", "")

            # Skip ChatInput/ChatOutput as they're handled specially
            if component_type in ["ChatInput", "ChatOutput"]:
                continue

            if node_id not in dependent_nodes and node_id in node_output_refs:
                leaf_nodes.append((node_id, component_type))

        # Build structured output based on leaf nodes
        if len(leaf_nodes) == 1:
            # Single leaf node - use it directly
            node_id, _ = leaf_nodes[0]
            builder.set_output(node_output_refs[node_id])
        elif len(leaf_nodes) > 1:
            # Multiple leaf nodes - create structured output using incremental building
            for node_id, component_type in leaf_nodes:
                # Generate a clean field name from the component type
                field_name = (
                    component_type.lower().replace("component", "").replace("_", "")
                )
                if not field_name:
                    field_name = node_id.lower().replace("-", "_")

                builder.add_output_field(field_name, node_output_refs[node_id])
        else:
            # No leaf nodes found - fallback to direct input passthrough
            builder.set_output(Value.input.add_path("message"))

    def _generate_flow_output(
        self,
        nodes: list[dict[str, Any]],
        dependencies: dict[str, list[str]],
        node_output_refs: dict[str, Any],
    ) -> Any:
        """Generate flow output using the new architecture.

        Args:
            nodes: Original Langflow nodes
            dependencies: Dependency graph
            node_output_refs: Mapping of node IDs to their output references

        Returns:
            Value for the flow output
        """
        # Look for ChatOutput nodes first
        chat_output_nodes = [
            n for n in nodes if n.get("data", {}).get("type") == "ChatOutput"
        ]

        if chat_output_nodes:
            # Use the first ChatOutput node
            chat_output_node = chat_output_nodes[0]
            node_id = chat_output_node["id"]

            # Check if ChatOutput has dependencies
            if node_id in dependencies and dependencies[node_id]:
                # ChatOutput depends on another node - use that node's output
                dep_node_id = dependencies[node_id][0]
                if dep_node_id in node_output_refs:
                    return node_output_refs[dep_node_id]

            # ChatOutput has no dependencies or dependencies not found - check if
            # it's a simple passthrough
            if chat_output_nodes and len(nodes) <= 2:
                # Simple ChatInput -> ChatOutput workflow
                chat_input_nodes = [
                    n for n in nodes if n.get("data", {}).get("type") == "ChatInput"
                ]
                if chat_input_nodes:
                    return Value.input("$.message")

        # Find leaf nodes (nodes with no dependents)
        dependent_nodes = set()
        for deps in dependencies.values():
            dependent_nodes.update(deps)

        leaf_nodes = []
        for node in nodes:
            node_id = node["id"]
            component_type = node.get("data", {}).get("type", "")

            # Skip ChatInput/ChatOutput as they're handled specially
            if component_type in ["ChatInput", "ChatOutput"]:
                continue

            if node_id not in dependent_nodes and node_id in node_output_refs:
                leaf_nodes.append(node_id)

        # If we have leaf nodes, use the first one
        if leaf_nodes:
            return node_output_refs[leaf_nodes[0]]

        # Fallback - use the last node with an output reference
        if node_output_refs:
            last_node_id = list(node_output_refs.keys())[-1]
            return node_output_refs[last_node_id]

        # Final fallback - direct input passthrough
        return Value.input("$.message")
