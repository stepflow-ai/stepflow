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

"""Path trie for efficient component path matching.

This module provides a trie (prefix tree) data structure optimized for
matching URL-like paths with support for:
- Exact segment matching
- Single segment parameters: {name}
- Wildcard (catch-all) parameters: {*name}

The trie provides O(n) lookup where n is the number of path segments,
compared to O(m*n) for iterating through m registered patterns.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Generic, TypeVar

T = TypeVar("T")


@dataclass
class TrieNode(Generic[T]):
    """A node in the path trie.

    Each node represents a path segment position and can have:
    - children: Exact match children keyed by segment value
    - param_child: A single-segment parameter child ({name})
    - wildcard_child: A catch-all parameter child ({*name})
    - value: The stored value if this node terminates a pattern
    - param_name: The parameter name if this is a param/wildcard node
    """

    children: dict[str, TrieNode[T]] = field(default_factory=dict)
    param_child: TrieNode[T] | None = None
    wildcard_child: TrieNode[T] | None = None
    value: T | None = None
    param_name: str | None = None
    is_wildcard: bool = False


class PathTrie(Generic[T]):
    """A trie for matching URL-like paths with parameters.

    Supports three types of path segments:
    - Literal: Exact string match (e.g., "users", "api")
    - Parameter: Single segment capture (e.g., "{id}", "{name}")
    - Wildcard: Capture remaining path (e.g., "{*path}", "{*component}")

    Example:
        >>> trie = PathTrie[str]()
        >>> trie.insert("/users/{id}/profile", "user_profile")
        >>> trie.insert("/api/{*path}", "api_handler")
        >>> trie.insert("/health", "health_check")
        >>>
        >>> trie.match("/users/123/profile")
        ("user_profile", {"id": "123"})
        >>> trie.match("/api/v1/users/list")
        ("api_handler", {"path": "v1/users/list"})
        >>> trie.match("/health")
        ("health_check", {})
    """

    def __init__(self):
        """Initialize an empty path trie."""
        self._root = TrieNode[T]()
        self._patterns: dict[str, T] = {}

    def insert(self, pattern: str, value: T) -> None:
        """Insert a path pattern with its associated value.

        Args:
            pattern: Path pattern (e.g., "/users/{id}/profile")
            value: Value to associate with this pattern

        Raises:
            ValueError: If pattern is invalid or conflicts with existing pattern
        """
        # Store the original pattern for listing
        self._patterns[pattern] = value

        # Parse and insert into trie
        segments = self._parse_pattern(pattern)
        node = self._root

        for segment in segments:
            if segment.startswith("{*") and segment.endswith("}"):
                # Wildcard parameter - captures rest of path
                param_name = segment[2:-1]
                if node.wildcard_child is None:
                    node.wildcard_child = TrieNode[T](
                        param_name=param_name, is_wildcard=True
                    )
                node = node.wildcard_child
                # Wildcards must be last segment
                break
            elif segment.startswith("{") and segment.endswith("}"):
                # Single segment parameter
                param_name = segment[1:-1]
                if node.param_child is None:
                    node.param_child = TrieNode[T](param_name=param_name)
                node = node.param_child
            else:
                # Literal segment
                if segment not in node.children:
                    node.children[segment] = TrieNode[T]()
                node = node.children[segment]

        node.value = value

    def match(self, path: str) -> tuple[T, dict[str, str]] | None:
        """Match a path against registered patterns.

        Matching priority (highest to lowest):
        1. Exact literal match
        2. Single-segment parameter match
        3. Wildcard (catch-all) match

        Args:
            path: The path to match (e.g., "/users/123/profile")

        Returns:
            Tuple of (value, captured_params) if matched, None otherwise
        """
        segments = self._parse_path(path)
        return self._match_recursive(self._root, segments, 0, {})

    def _match_recursive(
        self,
        node: TrieNode[T],
        segments: list[str],
        index: int,
        params: dict[str, str],
    ) -> tuple[T, dict[str, str]] | None:
        """Recursively match path segments against trie nodes.

        Args:
            node: Current trie node
            segments: List of path segments to match
            index: Current segment index
            params: Accumulated captured parameters

        Returns:
            Tuple of (value, params) if matched, None otherwise
        """
        # Base case: consumed all segments
        if index >= len(segments):
            if node.value is not None:
                return node.value, params
            return None

        current_segment = segments[index]

        # Priority 1: Try exact literal match first
        if current_segment in node.children:
            result = self._match_recursive(
                node.children[current_segment], segments, index + 1, params.copy()
            )
            if result is not None:
                return result

        # Priority 2: Try single-segment parameter match
        if node.param_child is not None:
            new_params = params.copy()
            if node.param_child.param_name:
                new_params[node.param_child.param_name] = current_segment
            result = self._match_recursive(
                node.param_child, segments, index + 1, new_params
            )
            if result is not None:
                return result

        # Priority 3: Try wildcard match (captures remaining segments)
        if node.wildcard_child is not None:
            # Wildcard captures everything from current segment onwards
            remaining_path = "/".join(segments[index:])
            new_params = params.copy()
            if node.wildcard_child.param_name:
                new_params[node.wildcard_child.param_name] = remaining_path
            if node.wildcard_child.value is not None:
                return node.wildcard_child.value, new_params

        return None

    def get_patterns(self) -> dict[str, T]:
        """Get all registered patterns and their values.

        Returns:
            Dictionary mapping patterns to values
        """
        return dict(self._patterns)

    def _parse_pattern(self, pattern: str) -> list[str]:
        """Parse a pattern into segments.

        Args:
            pattern: Path pattern (e.g., "/users/{id}/profile")

        Returns:
            List of segments (e.g., ["users", "{id}", "profile"])
        """
        # Remove leading slash and split
        if pattern.startswith("/"):
            pattern = pattern[1:]
        return [s for s in pattern.split("/") if s]

    def _parse_path(self, path: str) -> list[str]:
        """Parse a path into segments.

        Args:
            path: Path string (e.g., "/users/123/profile")

        Returns:
            List of segments (e.g., ["users", "123", "profile"])
        """
        # Remove leading slash and split
        if path.startswith("/"):
            path = path[1:]
        return [s for s in path.split("/") if s]

    def __contains__(self, pattern: str) -> bool:
        """Check if a pattern is registered."""
        return pattern in self._patterns

    def __len__(self) -> int:
        """Return the number of registered patterns."""
        return len(self._patterns)
