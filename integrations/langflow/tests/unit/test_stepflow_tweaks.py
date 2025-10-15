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

"""Tests for Stepflow-level tweaks functionality."""

import pytest

from stepflow_langflow_integration.converter.stepflow_tweaks import (
    apply_stepflow_tweaks,
)
from stepflow_langflow_integration.converter.translator import LangflowConverter
from tests.helpers.tweaks_builder import TweaksBuilder


class TestStepflowTweaks:
    """Test Stepflow-level tweaks functionality."""


class TestStepflowTweaksIntegration:
    """Test integration with the full Langflow conversion process."""

    @pytest.fixture
    def basic_prompting_flow(self):
        """Convert basic_prompting.json to a Flow object for testing."""
        import json
        from pathlib import Path

        converter = LangflowConverter()
        fixture_path = (
            Path(__file__).parent.parent
            / "fixtures"
            / "langflow"
            / "basic_prompting.json"
        )

        if fixture_path.exists():
            with open(fixture_path) as f:
                langflow_data = json.load(f)
            return converter.convert(langflow_data)
        else:
            pytest.skip("basic_prompting.json fixture not found")

    def test_real_workflow_tweaks_application(self, basic_prompting_flow):
        """Test tweaks application on a real converted workflow."""
        tweaks = {
            "LanguageModelComponent-kBOja": {  # Must match actual component ID
                "api_key": "integration_test_key",
                "temperature": 0.7,
                "model_name": "gpt-4",
            }
        }

        modified_flow = apply_stepflow_tweaks(basic_prompting_flow, tweaks)

        # Find the LanguageModelComponent UDF executor step
        langflow_step = None
        for step in modified_flow.steps:
            if (
                step.id == "langflow_LanguageModelComponent-kBOja"
                and step.component == "/langflow/udf_executor"
            ):
                langflow_step = step
                break

        assert langflow_step is not None, (
            "LanguageModelComponent UDF executor step not found"
        )

        # Verify tweaks were applied
        step_input = langflow_step.input["input"]
        assert step_input["api_key"] == "integration_test_key"
        assert step_input["temperature"] == 0.7
        assert step_input["model_name"] == "gpt-4"

        # Verify original inputs are still present
        assert "input_value" in step_input  # From workflow input
        assert "system_message" in step_input  # From prompt step


class TestTweaksBuilder:
    """Test the TweaksBuilder utility for creating tweaks."""

    def test_basic_tweak_building(self):
        """Test basic tweak creation with direct values."""
        builder = TweaksBuilder()
        builder.add_tweak("Component-123", "api_key", "test_key")
        builder.add_tweak("Component-123", "temperature", 0.8)
        builder.add_tweak("AnotherComponent-456", "model", "gpt-4")

        tweaks = builder.build()

        expected = {
            "Component-123": {"api_key": "test_key", "temperature": 0.8},
            "AnotherComponent-456": {"model": "gpt-4"},
        }

        assert tweaks == expected

    def test_method_chaining(self):
        """Test that methods can be chained for fluent API."""
        tweaks = (
            TweaksBuilder()
            .add_tweak("Component-123", "api_key", "test_key")
            .add_tweak("Component-123", "temperature", 0.8)
            .build()
        )

        expected = {"Component-123": {"api_key": "test_key", "temperature": 0.8}}

        assert tweaks == expected

    def test_env_tweak_with_existing_variable(self, monkeypatch):
        """Test adding tweaks from environment variables that exist."""
        monkeypatch.setenv("TEST_API_KEY", "env_test_key")
        monkeypatch.setenv("TEST_TEMPERATURE", "0.9")

        tweaks = (
            TweaksBuilder()
            .add_env_tweak("Component-123", "api_key", "TEST_API_KEY")
            .add_env_tweak("Component-123", "temperature", "TEST_TEMPERATURE")
            .build()
        )

        expected = {
            "Component-123": {
                "api_key": "env_test_key",
                "temperature": "0.9",  # Environment variables are strings
            }
        }

        assert tweaks == expected

    def test_env_tweak_with_missing_variable(self, monkeypatch):
        """Test that missing environment variables are tracked."""
        # Ensure the env var doesn't exist
        monkeypatch.delenv("MISSING_API_KEY", raising=False)

        builder = TweaksBuilder()
        builder.add_env_tweak("Component-123", "api_key", "MISSING_API_KEY")

        # Should track missing var
        assert "MISSING_API_KEY" in builder.missing_env_vars

        # Should raise error on build
        with pytest.raises(
            ValueError, match="Missing required environment variables: MISSING_API_KEY"
        ):
            builder.build()

    def test_build_or_skip_with_missing_env_vars(self, monkeypatch):
        """Test that build_or_skip skips test when env vars are missing."""
        monkeypatch.delenv("MISSING_VAR", raising=False)

        builder = TweaksBuilder()
        builder.add_env_tweak("Component-123", "field", "MISSING_VAR")

        # Should call pytest.skip
        with pytest.raises(
            pytest.skip.Exception,
            match="Missing required environment variables: MISSING_VAR",
        ):
            builder.build_or_skip()

    def test_build_or_skip_with_all_env_vars_present(self, monkeypatch):
        """Test that build_or_skip works normally when all env vars are present."""
        monkeypatch.setenv("PRESENT_VAR", "test_value")

        tweaks = (
            TweaksBuilder()
            .add_env_tweak("Component-123", "field", "PRESENT_VAR")
            .build_or_skip()
        )

        expected = {"Component-123": {"field": "test_value"}}

        assert tweaks == expected

    def test_add_openai_tweaks(self, monkeypatch):
        """Test the convenience method for OpenAI tweaks."""
        monkeypatch.setenv("OPENAI_API_KEY", "openai_test_key")

        tweaks = (
            TweaksBuilder()
            .add_openai_tweaks(
                "LanguageModelComponent-abc123", temperature=0.7, model_name="gpt-4"
            )
            .build()
        )

        expected = {
            "LanguageModelComponent-abc123": {
                "api_key": "openai_test_key",
                "temperature": 0.7,
                "model_name": "gpt-4",
            }
        }

        assert tweaks == expected

    def test_add_openai_tweaks_custom_env_var(self, monkeypatch):
        """Test OpenAI tweaks with custom environment variable name."""
        monkeypatch.setenv("CUSTOM_OPENAI_KEY", "custom_key")

        tweaks = (
            TweaksBuilder()
            .add_openai_tweaks(
                "LanguageModelComponent-abc123", api_key_env="CUSTOM_OPENAI_KEY"
            )
            .build()
        )

        expected = {"LanguageModelComponent-abc123": {"api_key": "custom_key"}}

        assert tweaks == expected

    def test_add_astradb_tweaks(self, monkeypatch):
        """Test the convenience method for AstraDB tweaks."""
        monkeypatch.setenv("ASTRA_DB_APPLICATION_TOKEN", "astra_token")
        monkeypatch.setenv("ASTRA_DB_API_ENDPOINT", "https://astra-endpoint.com")

        tweaks = TweaksBuilder().add_astradb_tweaks("AstraDB-store-123").build()

        expected = {
            "AstraDB-store-123": {
                "token": "astra_token",
                "api_endpoint": "https://astra-endpoint.com",
                "database_name": "langflow-test",  # Default value
                "collection_name": "test_collection",  # Default value
            }
        }

        assert tweaks == expected

    def test_add_astradb_tweaks_with_overrides(self, monkeypatch):
        """Test AstraDB tweaks with overridden default values."""
        monkeypatch.setenv("ASTRA_DB_APPLICATION_TOKEN", "astra_token")
        monkeypatch.setenv("ASTRA_DB_API_ENDPOINT", "https://astra-endpoint.com")

        tweaks = (
            TweaksBuilder()
            .add_astradb_tweaks(
                "AstraDB-store-123",
                database_name="custom_db",
                collection_name="custom_collection",
                extra_field="extra_value",
            )
            .build()
        )

        expected = {
            "AstraDB-store-123": {
                "token": "astra_token",
                "api_endpoint": "https://astra-endpoint.com",
                "database_name": "custom_db",
                "collection_name": "custom_collection",
                "extra_field": "extra_value",
            }
        }

        assert tweaks == expected

    def test_mixed_tweaks_and_env_vars(self, monkeypatch):
        """Test combining direct tweaks and environment variable tweaks."""
        monkeypatch.setenv("API_KEY", "env_api_key")

        tweaks = (
            TweaksBuilder()
            .add_env_tweak("Component-123", "api_key", "API_KEY")
            .add_tweak("Component-123", "temperature", 0.5)
            .add_tweak("Component-456", "model", "claude-3")
            .build()
        )

        expected = {
            "Component-123": {"api_key": "env_api_key", "temperature": 0.5},
            "Component-456": {"model": "claude-3"},
        }

        assert tweaks == expected

    def test_overwriting_tweaks(self):
        """Test that later tweaks overwrite earlier ones."""
        tweaks = (
            TweaksBuilder()
            .add_tweak("Component-123", "temperature", 0.3)
            .add_tweak("Component-123", "temperature", 0.7)  # Should overwrite
            .build()
        )

        expected = {"Component-123": {"temperature": 0.7}}

        assert tweaks == expected

    def test_empty_builder(self):
        """Test that empty builder produces empty tweaks."""
        tweaks = TweaksBuilder().build()
        assert tweaks == {}

    def test_multiple_missing_env_vars(self, monkeypatch):
        """Test error handling with multiple missing environment variables."""
        monkeypatch.delenv("MISSING_VAR_1", raising=False)
        monkeypatch.delenv("MISSING_VAR_2", raising=False)

        builder = TweaksBuilder()
        builder.add_env_tweak("Component-123", "field1", "MISSING_VAR_1")
        builder.add_env_tweak("Component-456", "field2", "MISSING_VAR_2")

        with pytest.raises(ValueError) as exc_info:
            builder.build()

        error_msg = str(exc_info.value)
        assert "MISSING_VAR_1" in error_msg
        assert "MISSING_VAR_2" in error_msg
