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

"""Integration tests for runtime tweaks functionality with real Langflow workflows."""

import pytest
import yaml

from stepflow_langflow_integration.converter.runtime_tweaks import (
    convert_langflow_tweaks_to_overrides,
    convert_langflow_tweaks_to_overrides_dict,
)
from stepflow_langflow_integration.converter.translator import LangflowConverter
from tests.helpers.tweaks_builder import TweaksBuilder


class TestRuntimeTweaksIntegration:
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

    @pytest.fixture 
    def basic_prompting_workflow_dict(self, basic_prompting_flow):
        """Get the basic prompting workflow as a dictionary."""
        converter = LangflowConverter()
        workflow_yaml = converter.to_yaml(basic_prompting_flow)
        return yaml.safe_load(workflow_yaml)

    def test_real_workflow_runtime_overrides_conversion(self, basic_prompting_flow, basic_prompting_workflow_dict):
        """Test runtime overrides conversion on a real converted workflow."""
        tweaks = {
            "LanguageModelComponent-kBOja": {  # Must match actual component ID
                "api_key": "runtime_override_key",
                "temperature": 0.7,
                "model_name": "gpt-4",
            }
        }

        # Test Flow-based conversion
        overrides_from_flow = convert_langflow_tweaks_to_overrides(basic_prompting_flow, tweaks)
        
        # Test dict-based conversion  
        overrides_from_dict = convert_langflow_tweaks_to_overrides_dict(basic_prompting_workflow_dict, tweaks)
        
        # Both should produce the same result
        assert overrides_from_flow == overrides_from_dict
        
        expected_overrides = {
            "stepOverrides": {
                "langflow_LanguageModelComponent-kBOja": {
                    "input.api_key": "runtime_override_key",
                    "input.temperature": 0.7,
                    "input.model_name": "gpt-4",
                }
            }
        }
        
        assert overrides_from_flow == expected_overrides
        assert overrides_from_dict == expected_overrides

    def test_runtime_overrides_format_validation(self, basic_prompting_workflow_dict):
        """Test that generated overrides have correct StepFlow format."""
        tweaks = {
            "LanguageModelComponent-kBOja": {
                "api_key": "test_key",
                "temperature": 0.8,
                "model": "gpt-3.5-turbo",
                "max_tokens": 1000,
                "custom_field": {"nested": "value"}
            }
        }

        overrides = convert_langflow_tweaks_to_overrides_dict(basic_prompting_workflow_dict, tweaks)
        
        # Check structure
        assert "stepOverrides" in overrides
        assert "langflow_LanguageModelComponent-kBOja" in overrides["stepOverrides"]
        
        step_overrides = overrides["stepOverrides"]["langflow_LanguageModelComponent-kBOja"]
        
        # All fields should be prefixed with "input."
        for key in step_overrides:
            assert key.startswith("input."), f"Override key '{key}' should start with 'input.'"
        
        # Check specific values
        assert step_overrides["input.api_key"] == "test_key"
        assert step_overrides["input.temperature"] == 0.8
        assert step_overrides["input.model"] == "gpt-3.5-turbo"
        assert step_overrides["input.max_tokens"] == 1000
        assert step_overrides["input.custom_field"] == {"nested": "value"}

    def test_multiple_component_overrides(self, basic_prompting_workflow_dict):
        """Test runtime overrides for multiple components."""
        tweaks = {
            "LanguageModelComponent-kBOja": {
                "api_key": "openai_key",
                "temperature": 0.7,
            },
            "SomeOtherComponent-xyz": {
                "param1": "value1",
                "param2": 42,
            }
        }

        # Note: This will only find the LanguageModelComponent since SomeOtherComponent-xyz doesn't exist
        # But with validation disabled, it should still generate overrides for both
        overrides = convert_langflow_tweaks_to_overrides_dict(basic_prompting_workflow_dict, tweaks, validate=False)
        
        expected = {
            "stepOverrides": {
                "langflow_LanguageModelComponent-kBOja": {
                    "input.api_key": "openai_key",
                    "input.temperature": 0.7,
                },
                "langflow_SomeOtherComponent-xyz": {
                    "input.param1": "value1", 
                    "input.param2": 42,
                }
            }
        }
        
        assert overrides == expected

    def test_empty_tweaks_handling(self, basic_prompting_workflow_dict):
        """Test handling of empty or None tweaks."""
        # Empty dict
        overrides = convert_langflow_tweaks_to_overrides_dict(basic_prompting_workflow_dict, {})
        assert overrides == {"stepOverrides": {}}
        
        # None
        overrides = convert_langflow_tweaks_to_overrides_dict(basic_prompting_workflow_dict, None)
        assert overrides == {"stepOverrides": {}}

    def test_validation_with_real_workflow(self, basic_prompting_workflow_dict):
        """Test validation against real workflow structure."""
        # Valid component ID
        valid_tweaks = {
            "LanguageModelComponent-kBOja": {"api_key": "test"}
        }
        
        # Should not raise
        overrides = convert_langflow_tweaks_to_overrides_dict(basic_prompting_workflow_dict, valid_tweaks, validate=True)
        assert overrides["stepOverrides"]["langflow_LanguageModelComponent-kBOja"]["input.api_key"] == "test"
        
        # Invalid component ID
        invalid_tweaks = {
            "NonExistentComponent-999": {"param": "value"}
        }
        
        # Should raise validation error
        with pytest.raises(ValueError) as exc_info:
            convert_langflow_tweaks_to_overrides_dict(basic_prompting_workflow_dict, invalid_tweaks, validate=True)
        
        assert "NonExistentComponent-999" in str(exc_info.value)
        assert "LanguageModelComponent-kBOja" in str(exc_info.value)  # Should show available components


class TestTweaksBuilder:
    """Test the TweaksBuilder utility for creating tweaks for runtime overrides."""

    def test_basic_tweak_building(self):
        """Test basic tweak creation with direct values."""
        builder = TweaksBuilder()
        builder.add_tweak("Component-123", "api_key", "test_key")
        builder.add_tweak("Component-123", "temperature", 0.8)
        builder.add_tweak("AnotherComponent-456", "model", "gpt-4")

        tweaks = builder.build()

        expected = {
            "Component-123": {
                "api_key": "test_key",
                "temperature": 0.8,
            },
            "AnotherComponent-456": {
                "model": "gpt-4",
            }
        }

        assert tweaks == expected

    def test_method_chaining(self):
        """Test builder pattern with method chaining."""
        tweaks = (
            TweaksBuilder()
            .add_tweak("Component-123", "api_key", "test_key")
            .add_tweak("Component-123", "temperature", 0.8) 
            .add_tweak("Component-456", "model", "gpt-4")
            .build()
        )

        expected = {
            "Component-123": {
                "api_key": "test_key",
                "temperature": 0.8,
            },
            "Component-456": {
                "model": "gpt-4",
            }
        }

        assert tweaks == expected

    def test_env_tweak_with_existing_variable(self, monkeypatch):
        """Test environment variable tweaks when variable exists."""
        monkeypatch.setenv("TEST_API_KEY", "env_test_key")

        builder = TweaksBuilder()
        builder.add_env_tweak("Component-123", "api_key", "TEST_API_KEY")

        tweaks = builder.build()

        expected = {
            "Component-123": {
                "api_key": "env_test_key",
            }
        }

        assert tweaks == expected

    def test_env_tweak_with_missing_variable(self):
        """Test environment variable tweaks when variable is missing."""
        builder = TweaksBuilder()
        builder.add_env_tweak("Component-123", "api_key", "MISSING_VAR")

        with pytest.raises(ValueError, match="Missing required environment variables"):
            builder.build()

    def test_build_or_skip_with_missing_env_vars(self):
        """Test build_or_skip when environment variables are missing."""
        builder = TweaksBuilder()
        builder.add_env_tweak("Component-123", "api_key", "MISSING_VAR")

        with pytest.raises(pytest.skip.Exception):
            builder.build_or_skip()

    def test_build_or_skip_with_all_env_vars_present(self, monkeypatch):
        """Test build_or_skip when all environment variables are present."""
        monkeypatch.setenv("TEST_API_KEY", "env_test_key")

        builder = TweaksBuilder()
        builder.add_env_tweak("Component-123", "api_key", "TEST_API_KEY")

        tweaks = builder.build_or_skip()

        expected = {
            "Component-123": {
                "api_key": "env_test_key",
            }
        }

        assert tweaks == expected

    def test_add_openai_tweaks(self, monkeypatch):
        """Test OpenAI-specific tweak helpers."""
        monkeypatch.setenv("OPENAI_API_KEY", "sk-test123")

        builder = TweaksBuilder()
        builder.add_openai_tweaks("LanguageModel-abc")

        tweaks = builder.build()

        expected = {
            "LanguageModel-abc": {
                "api_key": "sk-test123",
            }
        }

        assert tweaks == expected

    def test_add_openai_tweaks_custom_env_var(self, monkeypatch):
        """Test OpenAI tweaks with custom environment variable."""
        monkeypatch.setenv("CUSTOM_OPENAI_KEY", "sk-custom123")

        builder = TweaksBuilder()
        builder.add_openai_tweaks("LanguageModel-abc", api_key_env="CUSTOM_OPENAI_KEY")

        tweaks = builder.build()

        expected = {
            "LanguageModel-abc": {
                "api_key": "sk-custom123",
            }
        }

        assert tweaks == expected

    def test_add_astradb_tweaks(self, monkeypatch):
        """Test AstraDB-specific tweak helpers."""
        monkeypatch.setenv("ASTRA_DB_API_ENDPOINT", "https://test.datastax.com")
        monkeypatch.setenv("ASTRA_DB_APPLICATION_TOKEN", "token123")

        builder = TweaksBuilder()
        builder.add_astradb_tweaks("VectorStore-xyz")

        tweaks = builder.build()

        expected = {
            "VectorStore-xyz": {
                "api_endpoint": "https://test.datastax.com",
                "token": "token123",
                "database_name": "langflow-test",  # Default value added automatically
                "collection_name": "test_collection",  # Default value added automatically
            }
        }

        assert tweaks == expected

    def test_add_astradb_tweaks_with_overrides(self, monkeypatch):
        """Test AstraDB tweaks with additional parameter overrides."""
        monkeypatch.setenv("ASTRA_DB_API_ENDPOINT", "https://test.datastax.com")
        monkeypatch.setenv("ASTRA_DB_APPLICATION_TOKEN", "token123")

        builder = TweaksBuilder()
        builder.add_astradb_tweaks("VectorStore-xyz", collection_name="custom_collection")

        tweaks = builder.build()

        expected = {
            "VectorStore-xyz": {
                "api_endpoint": "https://test.datastax.com",
                "token": "token123",
                "database_name": "langflow-test",  # Default value
                "collection_name": "custom_collection",  # Override value
            }
        }

        assert tweaks == expected

    def test_mixed_tweaks_and_env_vars(self, monkeypatch):
        """Test mixing direct tweaks and environment variable tweaks."""
        monkeypatch.setenv("API_KEY", "env_key")

        builder = TweaksBuilder()
        builder.add_tweak("Component-123", "temperature", 0.7)
        builder.add_env_tweak("Component-123", "api_key", "API_KEY")
        builder.add_tweak("Component-456", "model", "gpt-4")

        tweaks = builder.build()

        expected = {
            "Component-123": {
                "temperature": 0.7,
                "api_key": "env_key",
            },
            "Component-456": {
                "model": "gpt-4",
            }
        }

        assert tweaks == expected

    def test_overwriting_tweaks(self):
        """Test that later tweaks overwrite earlier ones."""
        builder = TweaksBuilder()
        builder.add_tweak("Component-123", "temperature", 0.5)
        builder.add_tweak("Component-123", "temperature", 0.9)  # Should overwrite

        tweaks = builder.build()

        expected = {
            "Component-123": {
                "temperature": 0.9,  # Latest value
            }
        }

        assert tweaks == expected

    def test_empty_builder(self):
        """Test building empty tweaks."""
        builder = TweaksBuilder()
        tweaks = builder.build()

        assert tweaks == {}

    def test_multiple_missing_env_vars(self):
        """Test error message with multiple missing environment variables."""
        builder = TweaksBuilder()
        builder.add_env_tweak("Component-123", "api_key", "MISSING_KEY1")
        builder.add_env_tweak("Component-456", "secret", "MISSING_KEY2")

        with pytest.raises(ValueError) as exc_info:
            builder.build()

        error_msg = str(exc_info.value)
        assert "MISSING_KEY1" in error_msg
        assert "MISSING_KEY2" in error_msg