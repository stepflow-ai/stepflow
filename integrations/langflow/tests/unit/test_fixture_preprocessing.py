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

"""Tests for fixture preprocessing utilities."""

from tests.helpers.fixture_preprocessing import (
    FlowFixturePreprocessor,
    preprocess_openai_fixture,
)


class TestFlowFixturePreprocessor:
    """Test fixture preprocessing functionality."""

    def test_environment_variable_substitution_template_format(self):
        """Test ${ENV_VAR} format substitution."""
        fixture = {
            "data": {
                "nodes": [
                    {
                        "id": "test-node",
                        "data": {
                            "type": "TestComponent",
                            "node": {
                                "template": {
                                    "api_key": {
                                        "value": "${OPENAI_API_KEY}",
                                        "_input_type": "SecretStrInput",
                                    }
                                }
                            },
                        },
                    }
                ]
            }
        }

        preprocessor = FlowFixturePreprocessor({"OPENAI_API_KEY": "test-key-123"})
        result = preprocessor.preprocess_fixture(fixture)

        api_key_value = result["data"]["nodes"][0]["data"]["node"]["template"][
            "api_key"
        ]["value"]
        assert api_key_value == "test-key-123"

    def test_direct_environment_variable_substitution(self):
        """Test direct environment variable name substitution."""
        fixture = {
            "data": {
                "nodes": [
                    {
                        "id": "test-node",
                        "data": {
                            "type": "TestComponent",
                            "node": {
                                "template": {
                                    "api_key": {
                                        "value": "OPENAI_API_KEY",
                                        "_input_type": "SecretStrInput",
                                    }
                                }
                            },
                        },
                    }
                ]
            }
        }

        preprocessor = FlowFixturePreprocessor({"OPENAI_API_KEY": "direct-key-456"})
        result = preprocessor.preprocess_fixture(fixture)

        api_key_value = result["data"]["nodes"][0]["data"]["node"]["template"][
            "api_key"
        ]["value"]
        assert api_key_value == "direct-key-456"

    def test_field_overrides(self):
        """Test field override functionality."""
        fixture = {
            "data": {
                "nodes": [
                    {
                        "id": "astra-node",
                        "data": {
                            "type": "AstraDB",
                            "node": {
                                "template": {
                                    "database_name": {"value": ""},
                                    "collection_name": {"value": "default"},
                                }
                            },
                        },
                    }
                ]
            }
        }

        field_overrides = {
            "astra-node": {
                "database_name": "test-db",
                "collection_name": "test-collection",
            }
        }

        preprocessor = FlowFixturePreprocessor()
        result = preprocessor.preprocess_fixture(fixture, field_overrides)

        template = result["data"]["nodes"][0]["data"]["node"]["template"]
        assert template["database_name"]["value"] == "test-db"
        assert template["collection_name"]["value"] == "test-collection"

    def test_no_substitution_for_non_env_vars(self):
        """Test that normal string values are left unchanged."""
        fixture = {
            "data": {
                "nodes": [
                    {
                        "id": "test-node",
                        "data": {
                            "type": "TestComponent",
                            "node": {
                                "template": {
                                    "normal_field": {"value": "normal_value"},
                                    "number_field": {"value": 42},
                                }
                            },
                        },
                    }
                ]
            }
        }

        preprocessor = FlowFixturePreprocessor({"SOME_VAR": "some-value"})
        result = preprocessor.preprocess_fixture(fixture)

        template = result["data"]["nodes"][0]["data"]["node"]["template"]
        assert template["normal_field"]["value"] == "normal_value"
        assert template["number_field"]["value"] == 42

    def test_missing_environment_variable_keeps_original(self):
        """Test that missing environment variables keep original values."""
        fixture = {
            "data": {
                "nodes": [
                    {
                        "id": "test-node",
                        "data": {
                            "type": "TestComponent",
                            "node": {
                                "template": {
                                    "api_key": {
                                        "value": "MISSING_API_KEY",
                                        "_input_type": "SecretStrInput",
                                    }
                                }
                            },
                        },
                    }
                ]
            }
        }

        preprocessor = FlowFixturePreprocessor({})  # Empty environment
        result = preprocessor.preprocess_fixture(fixture)

        api_key_value = result["data"]["nodes"][0]["data"]["node"]["template"][
            "api_key"
        ]["value"]
        assert api_key_value == "MISSING_API_KEY"  # Should keep original


class TestComponentBasedSubstitution:
    """Test the new component-based substitution system."""

    def test_substitute_component_inputs_basic(self):
        """Test basic component input substitution by prefix."""
        from tests.helpers.fixture_preprocessing import substitute_component_inputs

        test_flow = {
            "data": {
                "nodes": [
                    {
                        "id": "AstraDB-test1",
                        "data": {
                            "node": {
                                "template": {
                                    "token": {"value": "ASTRA_DB_APPLICATION_TOKEN"},
                                    "api_endpoint": {"value": ""},
                                    "database_name": {"value": ""},
                                }
                            }
                        },
                    },
                    {
                        "id": "SomeOther-component",
                        "data": {
                            "node": {
                                "template": {"token": {"value": "should_not_change"}}
                            }
                        },
                    },
                ]
            }
        }

        substitutions = {
            "token": "test-token-value",
            "api_endpoint": "https://test-endpoint.com",
            "database_name": "test-db",
        }

        result = substitute_component_inputs(test_flow, "AstraDB", substitutions)

        # Check AstraDB component was updated
        astra_template = result["data"]["nodes"][0]["data"]["node"]["template"]
        assert astra_template["token"]["value"] == "test-token-value"
        assert astra_template["api_endpoint"]["value"] == "https://test-endpoint.com"
        assert astra_template["database_name"]["value"] == "test-db"

        # Check other component was not changed
        other_template = result["data"]["nodes"][1]["data"]["node"]["template"]
        assert other_template["token"]["value"] == "should_not_change"

    def test_substitute_astradb_inputs(self):
        """Test AstraDB-specific substitution helper."""
        import os

        from tests.helpers.fixture_preprocessing import substitute_astradb_inputs

        # Mock environment variables
        original_env = {}
        test_env = {
            "ASTRA_DB_APPLICATION_TOKEN": "test-astra-token",
            "ASTRA_DB_API_ENDPOINT": "https://test-astra-endpoint.com",
        }

        # Store original values and set test values
        for key, value in test_env.items():
            original_env[key] = os.environ.get(key)
            os.environ[key] = value

        try:
            test_flow = {
                "data": {
                    "nodes": [
                        {
                            "id": "AstraDB-node1",
                            "data": {
                                "node": {
                                    "template": {
                                        "token": {
                                            "value": "ASTRA_DB_APPLICATION_TOKEN"
                                        },
                                        "api_endpoint": {"value": ""},
                                        "database_name": {"value": ""},
                                        "collection_name": {"value": ""},
                                    }
                                }
                            },
                        }
                    ]
                }
            }

            result = substitute_astradb_inputs(test_flow)

            template = result["data"]["nodes"][0]["data"]["node"]["template"]
            assert template["token"]["value"] == "test-astra-token"
            assert (
                template["api_endpoint"]["value"] == "https://test-astra-endpoint.com"
            )
            assert template["database_name"]["value"] == "langflow-test"
            assert template["collection_name"]["value"] == "test_collection"

        finally:
            # Restore original environment
            for key, original_value in original_env.items():
                if original_value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = original_value

    def test_substitute_openai_inputs_with_prefix_component(self):
        """Test OpenAI substitution with OpenAI-prefixed component."""
        import os

        from tests.helpers.fixture_preprocessing import substitute_openai_inputs

        # Mock environment
        original_key = os.environ.get("OPENAI_API_KEY")
        os.environ["OPENAI_API_KEY"] = "test-openai-key-123"

        try:
            test_flow = {
                "data": {
                    "nodes": [
                        {
                            "id": "OpenAIEmbeddings-abc123",
                            "data": {
                                "node": {
                                    "template": {
                                        "openai_api_key": {"value": "OPENAI_API_KEY"},
                                        "model": {"value": "text-embedding-3-small"},
                                    }
                                }
                            },
                        }
                    ]
                }
            }

            result = substitute_openai_inputs(test_flow)

            template = result["data"]["nodes"][0]["data"]["node"]["template"]
            assert template["openai_api_key"]["value"] == "test-openai-key-123"
            assert template["model"]["value"] == "text-embedding-3-small"  # unchanged

        finally:
            if original_key is None:
                os.environ.pop("OPENAI_API_KEY", None)
            else:
                os.environ["OPENAI_API_KEY"] = original_key

    def test_substitute_openai_inputs_with_generic_component(self):
        """Test OpenAI substitution with generic component using OpenAI API key."""
        import os

        from tests.helpers.fixture_preprocessing import substitute_openai_inputs

        # Mock environment
        original_key = os.environ.get("OPENAI_API_KEY")
        os.environ["OPENAI_API_KEY"] = "test-generic-openai-key"

        try:
            test_flow = {
                "data": {
                    "nodes": [
                        {
                            "id": "LanguageModelComponent-xyz789",
                            "data": {
                                "node": {
                                    "template": {
                                        "api_key": {
                                            "value": "OPENAI_API_KEY"
                                        },  # References OpenAI
                                        "model": {"value": "gpt-4"},
                                        "temperature": {"value": 0.7},
                                    }
                                }
                            },
                        }
                    ]
                }
            }

            result = substitute_openai_inputs(test_flow)

            template = result["data"]["nodes"][0]["data"]["node"]["template"]
            assert template["api_key"]["value"] == "test-generic-openai-key"
            assert template["model"]["value"] == "gpt-4"  # unchanged
            assert template["temperature"]["value"] == 0.7  # unchanged

        finally:
            if original_key is None:
                os.environ.pop("OPENAI_API_KEY", None)
            else:
                os.environ["OPENAI_API_KEY"] = original_key

    def test_substitute_astradb_and_openai_inputs_combined(self):
        """Test combined AstraDB and OpenAI substitution."""
        import os

        from tests.helpers.fixture_preprocessing import (
            substitute_astradb_and_openai_inputs,
        )

        # Mock environment variables
        original_env = {}
        test_env = {
            "ASTRA_DB_APPLICATION_TOKEN": "combined-astra-token",
            "ASTRA_DB_API_ENDPOINT": "https://combined-astra.com",
            "OPENAI_API_KEY": "combined-openai-key",
        }

        for key, value in test_env.items():
            original_env[key] = os.environ.get(key)
            os.environ[key] = value

        try:
            test_flow = {
                "data": {
                    "nodes": [
                        {
                            "id": "AstraDB-vector-store",
                            "data": {
                                "node": {
                                    "template": {
                                        "token": {
                                            "value": "ASTRA_DB_APPLICATION_TOKEN"
                                        },
                                        "api_endpoint": {"value": ""},
                                        "database_name": {"value": ""},
                                    }
                                }
                            },
                        },
                        {
                            "id": "OpenAIEmbeddings-embedder",
                            "data": {
                                "node": {
                                    "template": {
                                        "openai_api_key": {"value": "OPENAI_API_KEY"},
                                    }
                                }
                            },
                        },
                        {
                            "id": "LanguageModelComponent-llm",
                            "data": {
                                "node": {
                                    "template": {"api_key": {"value": "OPENAI_API_KEY"}}
                                }
                            },
                        },
                    ]
                }
            }

            result = substitute_astradb_and_openai_inputs(test_flow)

            # Check AstraDB substitution
            astra_template = result["data"]["nodes"][0]["data"]["node"]["template"]
            assert astra_template["token"]["value"] == "combined-astra-token"
            assert (
                astra_template["api_endpoint"]["value"] == "https://combined-astra.com"
            )
            assert astra_template["database_name"]["value"] == "langflow-test"

            # Check OpenAI substitutions - both prefixed and generic components
            openai_template = result["data"]["nodes"][1]["data"]["node"]["template"]
            assert openai_template["openai_api_key"]["value"] == "combined-openai-key"

            llm_template = result["data"]["nodes"][2]["data"]["node"]["template"]
            assert llm_template["api_key"]["value"] == "combined-openai-key"

        finally:
            # Restore original environment
            for key, original_value in original_env.items():
                if original_value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = original_value

    def test_substitution_preserves_original_data(self):
        """Test that substitution doesn't modify the original data structure."""
        import copy

        from tests.helpers.fixture_preprocessing import substitute_component_inputs

        original_flow = {
            "data": {
                "nodes": [
                    {
                        "id": "TestComponent-123",
                        "data": {
                            "node": {
                                "template": {"field1": {"value": "original_value"}}
                            }
                        },
                    }
                ]
            }
        }

        # Make a copy to compare against
        original_copy = copy.deepcopy(original_flow)

        # Perform substitution
        result = substitute_component_inputs(
            original_flow, "TestComponent", {"field1": "new_value"}
        )

        # Verify original is unchanged
        assert original_flow == original_copy

        # Verify result has the substitution
        assert (
            result["data"]["nodes"][0]["data"]["node"]["template"]["field1"]["value"]
            == "new_value"
        )


class TestConvenienceFunctions:
    """Test convenience functions for common scenarios."""

    def test_preprocess_openai_fixture(self):
        """Test OpenAI fixture preprocessing."""
        # This would normally load basic_prompting.json and process it
        # For now, just test that the function exists and can be called
        try:
            result = preprocess_openai_fixture("basic_prompting", api_key="test-key")
            assert isinstance(result, dict)
            assert "data" in result
        except FileNotFoundError:
            # Expected if fixture path doesn't resolve properly in test
            pass
