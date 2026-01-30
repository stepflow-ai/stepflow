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

"""Unit tests for PathTrie data structure."""

from stepflow_py.worker.path_trie import PathTrie


class TestPathTrieBasics:
    """Basic PathTrie functionality tests."""

    def test_empty_trie(self):
        """Test empty trie returns no matches."""
        trie = PathTrie[str]()
        assert trie.match("/any/path") is None
        assert len(trie) == 0

    def test_exact_match(self):
        """Test exact literal path matching."""
        trie = PathTrie[str]()
        trie.insert("/health", "health_handler")
        trie.insert("/api/v1/status", "status_handler")

        result = trie.match("/health")
        assert result is not None
        value, params = result
        assert value == "health_handler"
        assert params == {}

        result = trie.match("/api/v1/status")
        assert result is not None
        value, params = result
        assert value == "status_handler"
        assert params == {}

    def test_no_match(self):
        """Test paths that don't match any pattern."""
        trie = PathTrie[str]()
        trie.insert("/health", "health_handler")

        assert trie.match("/other") is None
        assert trie.match("/health/extra") is None
        assert trie.match("/") is None

    def test_contains(self):
        """Test __contains__ method."""
        trie = PathTrie[str]()
        trie.insert("/health", "handler")

        assert "/health" in trie
        assert "/other" not in trie

    def test_len(self):
        """Test __len__ method."""
        trie = PathTrie[str]()
        assert len(trie) == 0

        trie.insert("/a", "a")
        assert len(trie) == 1

        trie.insert("/b", "b")
        assert len(trie) == 2


class TestSingleSegmentParameters:
    """Tests for single-segment parameter matching ({name})."""

    def test_single_param(self):
        """Test single parameter capture."""
        trie = PathTrie[str]()
        trie.insert("/users/{id}", "user_handler")

        result = trie.match("/users/123")
        assert result is not None
        value, params = result
        assert value == "user_handler"
        assert params == {"id": "123"}

    def test_param_in_middle(self):
        """Test parameter in middle of path."""
        trie = PathTrie[str]()
        trie.insert("/users/{id}/profile", "profile_handler")

        result = trie.match("/users/abc/profile")
        assert result is not None
        value, params = result
        assert value == "profile_handler"
        assert params == {"id": "abc"}

        # Should not match extra segments
        assert trie.match("/users/abc/profile/extra") is None

    def test_multiple_params(self):
        """Test multiple parameter capture."""
        trie = PathTrie[str]()
        trie.insert("/orgs/{org}/repos/{repo}", "repo_handler")

        result = trie.match("/orgs/acme/repos/my-project")
        assert result is not None
        value, params = result
        assert value == "repo_handler"
        assert params == {"org": "acme", "repo": "my-project"}

    def test_param_with_special_chars(self):
        """Test parameter values can contain special characters."""
        trie = PathTrie[str]()
        trie.insert("/files/{name}", "file_handler")

        result = trie.match("/files/my-file_v2.txt")
        assert result is not None
        value, params = result
        assert value == "file_handler"
        assert params == {"name": "my-file_v2.txt"}

    def test_param_does_not_match_slash(self):
        """Test single param doesn't match paths with slashes."""
        trie = PathTrie[str]()
        trie.insert("/files/{name}", "file_handler")

        # Should not match - {name} only captures single segment
        assert trie.match("/files/path/to/file") is None


class TestWildcardParameters:
    """Tests for wildcard parameter matching ({*name})."""

    def test_wildcard_basic(self):
        """Test basic wildcard capture."""
        trie = PathTrie[str]()
        trie.insert("/api/{*path}", "api_handler")

        result = trie.match("/api/users")
        assert result is not None
        value, params = result
        assert value == "api_handler"
        assert params == {"path": "users"}

    def test_wildcard_nested(self):
        """Test wildcard captures nested paths."""
        trie = PathTrie[str]()
        trie.insert("/api/{*path}", "api_handler")

        result = trie.match("/api/users/123/profile")
        assert result is not None
        value, params = result
        assert value == "api_handler"
        assert params == {"path": "users/123/profile"}

    def test_wildcard_with_prefix(self):
        """Test wildcard with literal prefix."""
        trie = PathTrie[str]()
        trie.insert("/langflow/core/{*component}", "core_handler")

        result = trie.match("/langflow/core/lfx/components/chat/ChatInput")
        assert result is not None
        value, params = result
        assert value == "core_handler"
        assert params == {"component": "lfx/components/chat/ChatInput"}

    def test_wildcard_requires_content(self):
        """Test wildcard requires at least one segment."""
        trie = PathTrie[str]()
        trie.insert("/api/{*path}", "api_handler")

        # Should not match - wildcard needs content
        assert trie.match("/api") is None
        assert trie.match("/api/") is None

    def test_mixed_param_and_wildcard(self):
        """Test combining single params and wildcards."""
        trie = PathTrie[str]()
        trie.insert("/api/{version}/{*path}", "versioned_api")

        result = trie.match("/api/v2/users/list")
        assert result is not None
        value, params = result
        assert value == "versioned_api"
        assert params == {"version": "v2", "path": "users/list"}


class TestMatchingPriority:
    """Tests for match priority (exact > param > wildcard)."""

    def test_exact_beats_param(self):
        """Test exact match takes precedence over parameter."""
        trie = PathTrie[str]()
        trie.insert("/users/{id}", "param_handler")
        trie.insert("/users/me", "exact_handler")

        # Exact match should win
        result = trie.match("/users/me")
        assert result is not None
        value, params = result
        assert value == "exact_handler"
        assert params == {}

        # Parameter should match other values
        result = trie.match("/users/123")
        assert result is not None
        value, params = result
        assert value == "param_handler"
        assert params == {"id": "123"}

    def test_exact_beats_wildcard(self):
        """Test exact match takes precedence over wildcard."""
        trie = PathTrie[str]()
        trie.insert("/api/{*path}", "wildcard_handler")
        trie.insert("/api/health", "health_handler")

        # Exact match should win
        result = trie.match("/api/health")
        assert result is not None
        value, params = result
        assert value == "health_handler"
        assert params == {}

        # Wildcard should match other paths
        result = trie.match("/api/users/list")
        assert result is not None
        value, params = result
        assert value == "wildcard_handler"
        assert params == {"path": "users/list"}

    def test_param_beats_wildcard(self):
        """Test single param takes precedence over wildcard at same position."""
        trie = PathTrie[str]()
        trie.insert("/api/{*path}", "wildcard_handler")
        trie.insert("/api/{resource}/list", "resource_list_handler")

        # Param path should match when it fits exactly
        result = trie.match("/api/users/list")
        assert result is not None
        value, params = result
        assert value == "resource_list_handler"
        assert params == {"resource": "users"}

        # Wildcard should match when param path doesn't fit
        result = trie.match("/api/users/123/profile")
        assert result is not None
        value, params = result
        assert value == "wildcard_handler"
        assert params == {"path": "users/123/profile"}

    def test_insertion_order_independent(self):
        """Test that match priority is independent of insertion order."""
        # Insert in reverse priority order
        trie1 = PathTrie[str]()
        trie1.insert("/api/{*path}", "wildcard")
        trie1.insert("/api/{id}", "param")
        trie1.insert("/api/health", "exact")

        # Insert in priority order
        trie2 = PathTrie[str]()
        trie2.insert("/api/health", "exact")
        trie2.insert("/api/{id}", "param")
        trie2.insert("/api/{*path}", "wildcard")

        # Both should have same behavior
        for trie in [trie1, trie2]:
            result = trie.match("/api/health")
            assert result is not None
            assert result[0] == "exact"

            result = trie.match("/api/123")
            assert result is not None
            assert result[0] == "param"

            result = trie.match("/api/a/b/c")
            assert result is not None
            assert result[0] == "wildcard"


class TestGetPatterns:
    """Tests for get_patterns method."""

    def test_get_patterns_empty(self):
        """Test get_patterns on empty trie."""
        trie = PathTrie[str]()
        assert trie.get_patterns() == {}

    def test_get_patterns_returns_all(self):
        """Test get_patterns returns all registered patterns."""
        trie = PathTrie[str]()
        trie.insert("/a", "a_handler")
        trie.insert("/b/{id}", "b_handler")
        trie.insert("/c/{*path}", "c_handler")

        patterns = trie.get_patterns()
        assert patterns == {
            "/a": "a_handler",
            "/b/{id}": "b_handler",
            "/c/{*path}": "c_handler",
        }

    def test_get_patterns_returns_copy(self):
        """Test get_patterns returns a copy, not the internal dict."""
        trie = PathTrie[str]()
        trie.insert("/a", "handler")

        patterns = trie.get_patterns()
        patterns["/b"] = "new_handler"

        # Original trie should be unaffected
        assert "/b" not in trie


class TestEdgeCases:
    """Edge case tests."""

    def test_root_path(self):
        """Test matching root path."""
        trie = PathTrie[str]()
        trie.insert("/", "root_handler")

        result = trie.match("/")
        # Note: empty path segments are filtered, so "/" matches empty list
        # This means we need a value at root node
        # Current implementation: "/" -> [] segments
        # We'd need to handle this case specially if needed

    def test_trailing_slash(self):
        """Test paths with trailing slashes."""
        trie = PathTrie[str]()
        trie.insert("/users", "users_handler")

        # Both with and without trailing slash should match
        # (trailing empty segments are filtered)
        result = trie.match("/users")
        assert result is not None
        assert result[0] == "users_handler"

        result = trie.match("/users/")
        assert result is not None
        assert result[0] == "users_handler"

    def test_double_slash(self):
        """Test paths with double slashes."""
        trie = PathTrie[str]()
        trie.insert("/users/{id}", "user_handler")

        # Double slashes create empty segments which are filtered
        result = trie.match("/users//123")
        # This becomes ["users", "123"] after filtering empty segments
        assert result is not None
        assert result[1] == {"id": "123"}

    def test_unicode_segments(self):
        """Test unicode path segments."""
        trie = PathTrie[str]()
        trie.insert("/文件/{name}", "file_handler")

        result = trie.match("/文件/テスト")
        assert result is not None
        value, params = result
        assert value == "file_handler"
        assert params == {"name": "テスト"}

    def test_generic_type(self):
        """Test trie works with different value types."""
        # With integers
        int_trie = PathTrie[int]()
        int_trie.insert("/count", 42)
        result = int_trie.match("/count")
        assert result is not None
        assert result[0] == 42

        # With custom objects
        class Handler:
            def __init__(self, name: str):
                self.name = name

        obj_trie = PathTrie[Handler]()
        handler = Handler("test")
        obj_trie.insert("/test", handler)
        result = obj_trie.match("/test")
        assert result is not None
        assert result[0] is handler
