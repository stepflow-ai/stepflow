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

"""Validation utilities for workflows and schemas."""

from typing import Dict, Any, List
from jsonschema import validate, ValidationError as JsonSchemaValidationError

from .errors import ValidationError


def validate_langflow_json(data: Dict[str, Any]) -> None:
    """Validate basic Langflow JSON structure.

    Args:
        data: Langflow JSON data

    Raises:
        ValidationError: If validation fails
    """
    if not isinstance(data, dict):
        raise ValidationError("Langflow data must be a dictionary")

    if "data" not in data:
        raise ValidationError("Missing 'data' key in Langflow JSON")

    langflow_data = data["data"]
    if not isinstance(langflow_data, dict):
        raise ValidationError("'data' must be a dictionary")

    if "nodes" not in langflow_data:
        raise ValidationError("Missing 'nodes' in Langflow data")

    if "edges" not in langflow_data:
        raise ValidationError("Missing 'edges' in Langflow data")

    nodes = langflow_data["nodes"]
    edges = langflow_data["edges"]

    if not isinstance(nodes, list):
        raise ValidationError("'nodes' must be a list")

    if not isinstance(edges, list):
        raise ValidationError("'edges' must be a list")

    # Validate individual nodes
    for i, node in enumerate(nodes):
        if not isinstance(node, dict):
            raise ValidationError(f"Node {i} must be a dictionary")

        if "id" not in node:
            raise ValidationError(f"Node {i} missing 'id'")

        if "data" not in node:
            raise ValidationError(f"Node {i} missing 'data'")


def validate_json_schema(instance: Any, schema: Dict[str, Any]) -> None:
    """Validate instance against JSON schema.

    Args:
        instance: Data to validate
        schema: JSON schema

    Raises:
        ValidationError: If validation fails
    """
    try:
        validate(instance=instance, schema=schema)
    except JsonSchemaValidationError as e:
        raise ValidationError(f"Schema validation failed: {e.message}")


def validate_step_dependencies(
    steps: List[Dict[str, Any]], dependencies: Dict[str, List[str]]
) -> None:
    """Validate that step dependencies reference valid steps.

    Args:
        steps: List of workflow steps
        dependencies: Dependency mapping

    Raises:
        ValidationError: If validation fails
    """
    step_ids = {step["id"] for step in steps}

    for target, sources in dependencies.items():
        if target not in step_ids:
            raise ValidationError(f"Dependency target '{target}' not found in steps")

        for source in sources:
            if source not in step_ids:
                raise ValidationError(
                    f"Dependency source '{source}' not found in steps"
                )


def check_circular_dependencies(dependencies: Dict[str, List[str]]) -> None:
    """Check for circular dependencies using DFS.

    Args:
        dependencies: Dependency mapping

    Raises:
        ValidationError: If circular dependencies found
    """
    # Build adjacency list
    graph = {}
    all_nodes = set()

    for target, sources in dependencies.items():
        all_nodes.add(target)
        for source in sources:
            all_nodes.add(source)
            if source not in graph:
                graph[source] = []
            graph[source].append(target)

    # DFS to detect cycles
    visited = set()
    rec_stack = set()

    def has_cycle(node: str) -> bool:
        visited.add(node)
        rec_stack.add(node)

        for neighbor in graph.get(node, []):
            if neighbor not in visited:
                if has_cycle(neighbor):
                    return True
            elif neighbor in rec_stack:
                return True

        rec_stack.remove(node)
        return False

    for node in all_nodes:
        if node not in visited:
            if has_cycle(node):
                raise ValidationError("Circular dependencies detected in workflow")
