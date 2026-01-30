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

"""Unit tests for the BaseExecutor shared functionality."""

from typing import Any
from unittest.mock import MagicMock

import pytest

from stepflow_langflow_integration.exceptions import ExecutionError
from stepflow_langflow_integration.executor.base_executor import BaseExecutor


class ConcreteTestExecutor(BaseExecutor):
    """Concrete implementation of BaseExecutor for testing."""

    async def _instantiate_component(
        self,
        component_info: dict[str, Any],
    ) -> tuple[Any, str]:
        """Test implementation that just returns the info as-is."""
        return component_info.get("instance"), component_info.get("name", "test")


@pytest.fixture
def executor():
    """Create a ConcreteTestExecutor instance for testing base functionality."""
    return ConcreteTestExecutor()


class TestBaseExecutorEnvVarResolution:
    """Tests for _resolve_env_variables in BaseExecutor."""

    def test_resolve_env_var_when_load_from_db(self, executor, monkeypatch):
        """Test that env vars are resolved for load_from_db fields."""
        monkeypatch.setenv("OPENAI_API_KEY", "sk-test-key-123")

        parameters = {"api_key": ""}
        template = {
            "api_key": {
                "value": "OPENAI_API_KEY",
                "load_from_db": True,
            },
        }

        result = executor._resolve_env_variables(parameters, template)

        assert result["api_key"] == "sk-test-key-123"

    def test_no_resolution_when_value_provided(self, executor, monkeypatch):
        """Test that env vars are NOT resolved when value is already set."""
        monkeypatch.setenv("OPENAI_API_KEY", "env-value")

        parameters = {"api_key": "already-set-value"}
        template = {
            "api_key": {
                "value": "OPENAI_API_KEY",
                "load_from_db": True,
            },
        }

        result = executor._resolve_env_variables(parameters, template)

        # Should keep the existing value
        assert result["api_key"] == "already-set-value"

    def test_no_resolution_without_load_from_db(self, executor, monkeypatch):
        """Test that env vars are NOT resolved without load_from_db flag."""
        monkeypatch.setenv("SOME_VAR", "env-value")

        parameters = {"param": ""}
        template = {
            "param": {
                "value": "SOME_VAR",
                # No load_from_db flag
            },
        }

        result = executor._resolve_env_variables(parameters, template)

        # Should keep empty value
        assert result["param"] == ""

    def test_error_when_env_var_missing(self, executor, monkeypatch):
        """Test that missing env var raises ExecutionError."""
        monkeypatch.delenv("MISSING_VAR", raising=False)

        parameters = {"api_key": ""}
        template = {
            "api_key": {
                "value": "MISSING_VAR",
                "load_from_db": True,
            },
        }

        with pytest.raises(ExecutionError, match="Environment variable.*not set"):
            executor._resolve_env_variables(parameters, template)

    def test_multiple_env_vars(self, executor, monkeypatch):
        """Test resolving multiple env vars."""
        monkeypatch.setenv("API_KEY_1", "key1")
        monkeypatch.setenv("API_KEY_2", "key2")

        parameters = {"key1": "", "key2": "", "regular": "value"}
        template = {
            "key1": {"value": "API_KEY_1", "load_from_db": True},
            "key2": {"value": "API_KEY_2", "load_from_db": True},
            "regular": {"value": "default"},
        }

        result = executor._resolve_env_variables(parameters, template)

        assert result["key1"] == "key1"
        assert result["key2"] == "key2"
        assert result["regular"] == "value"  # Unchanged

    def test_non_dict_template_field_skipped(self, executor):
        """Test that non-dict template fields are skipped gracefully."""
        parameters = {"param": ""}
        template = {
            "param": "direct_value",  # Not a dict
        }

        result = executor._resolve_env_variables(parameters, template)

        assert result["param"] == ""  # Unchanged


class TestBaseExecutorDetermineExecutionMethod:
    """Tests for _determine_execution_method in BaseExecutor."""

    def test_with_selected_output_match(self, executor):
        """Test finding method for matching selected_output."""
        outputs = [
            {"name": "text", "method": "text_response"},
            {"name": "message", "method": "build_message"},
        ]
        result = executor._determine_execution_method(outputs, "message")
        assert result == "build_message"

    def test_fallback_to_first(self, executor):
        """Test fallback to first output's method."""
        outputs = [
            {"name": "default", "method": "default_method"},
            {"name": "other", "method": "other_method"},
        ]
        result = executor._determine_execution_method(outputs, None)
        assert result == "default_method"

    def test_empty_outputs(self, executor):
        """Test with empty outputs list."""
        result = executor._determine_execution_method([], None)
        assert result is None

    def test_selected_not_found_fallback(self, executor):
        """Test fallback when selected_output doesn't match."""
        outputs = [{"name": "text", "method": "text_method"}]
        result = executor._determine_execution_method(outputs, "nonexistent")
        assert result == "text_method"


class TestBaseExecutorApplyInputDefaults:
    """Tests for _apply_component_input_defaults in BaseExecutor."""

    def test_no_inputs_attribute(self, executor):
        """Test with component that has no inputs attribute."""
        component = MagicMock(spec=[])  # No inputs attribute
        params = {"key": "value"}
        result = executor._apply_component_input_defaults(component, params)
        assert result == {"key": "value"}

    def test_adds_missing_defaults(self, executor):
        """Test that defaults are added for missing parameters."""
        input_def = MagicMock()
        input_def.name = "temperature"
        input_def.value = 0.7

        component = MagicMock()
        component.inputs = [input_def]

        params = {"model": "gpt-4"}
        result = executor._apply_component_input_defaults(component, params)
        assert result == {"model": "gpt-4", "temperature": 0.7}

    def test_preserves_existing_values(self, executor):
        """Test that existing params are not overwritten."""
        input_def = MagicMock()
        input_def.name = "temperature"
        input_def.value = 0.7

        component = MagicMock()
        component.inputs = [input_def]

        params = {"temperature": 0.9}  # Already set
        result = executor._apply_component_input_defaults(component, params)
        assert result == {"temperature": 0.9}  # Original preserved


class TestBaseExecutorSetupGraphContext:
    """Tests for _setup_graph_context in BaseExecutor."""

    def test_sets_graph_context(self, executor):
        """Test that graph context is set correctly."""
        component = MagicMock()
        component.__dict__ = {}

        executor._setup_graph_context(component, "test-session-id")

        assert "graph" in component.__dict__
        assert component.__dict__["graph"].session_id == "test-session-id"
        assert component.__dict__["graph"].vertices == []
        assert component.__dict__["graph"].flow_id is None
