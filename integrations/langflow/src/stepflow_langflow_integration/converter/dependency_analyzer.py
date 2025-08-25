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

"""Dependency analysis for Langflow workflows."""

from typing import Dict, Any, List


class DependencyAnalyzer:
    """Analyzes Langflow edges to build step dependency graphs."""

    def build_dependency_graph(
        self, edges: List[Dict[str, Any]]
    ) -> Dict[str, List[str]]:
        """Build a dependency graph from Langflow edges.

        Args:
            edges: List of Langflow edge objects

        Returns:
            Dict mapping step IDs to their dependencies
        """
        dependencies = {}

        for edge in edges:
            source = edge.get("source")
            target = edge.get("target")

            if source and target:
                if target not in dependencies:
                    dependencies[target] = []
                dependencies[target].append(source)

        return dependencies

    def get_execution_order(self, dependencies: Dict[str, List[str]]) -> List[str]:
        """Get topological execution order for nodes.

        Args:
            dependencies: Dependency graph

        Returns:
            List of node IDs in execution order

        Raises:
            ValueError: If circular dependencies are detected
        """
        # Topological sort using Kahn's algorithm
        in_degree = {}
        all_nodes = set()

        # Collect all nodes and calculate in-degrees
        for target, sources in dependencies.items():
            all_nodes.add(target)
            in_degree[target] = in_degree.get(target, 0) + len(sources)
            for source in sources:
                all_nodes.add(source)
                if source not in in_degree:
                    in_degree[source] = 0

        # Find nodes with no incoming edges
        queue = [node for node in all_nodes if in_degree[node] == 0]
        result = []

        while queue:
            node = queue.pop(0)
            result.append(node)

            # Update in-degrees for dependent nodes
            for target, sources in dependencies.items():
                if node in sources:
                    in_degree[target] -= 1
                    if in_degree[target] == 0:
                        queue.append(target)

        # Check for circular dependencies
        if len(result) != len(all_nodes):
            raise ValueError("Circular dependencies detected in workflow")

        return result
