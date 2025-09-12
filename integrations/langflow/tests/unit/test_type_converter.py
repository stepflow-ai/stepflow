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

"""Unit tests for TypeConverter BaseModel serialization and deserialization."""

import os
from unittest.mock import patch

import pytest
from pydantic import BaseModel, SecretStr

from stepflow_langflow_integration.executor.type_converter import TypeConverter


# Test BaseModel classes
class SimpleTestModel(BaseModel):
    """Simple test model without special types."""

    name: str
    value: int
    enabled: bool = True


class SecretTestModel(BaseModel):
    """Test model with SecretStr fields."""

    name: str
    api_key: SecretStr
    optional_secret: SecretStr | None = None


class MockOpenAIEmbeddings(BaseModel):
    """Mock OpenAI embeddings model to test real-world scenario."""

    model: str = "text-embedding-3-small"
    openai_api_key: SecretStr
    chunk_size: int = 1000
    max_retries: int = 3
    dimensions: int | None = None


class TestTypeConverter:
    """Test cases for TypeConverter BaseModel serialization."""

    def setup_method(self):
        """Set up test fixtures."""
        self.converter = TypeConverter()

    def test_simple_types_serialization(self):
        """Test that simple types are not affected by BaseModel serialization."""
        # Test simple types pass through unchanged
        assert self.converter.serialize_langflow_object("test") == "test"
        assert self.converter.serialize_langflow_object(42) == 42
        assert self.converter.serialize_langflow_object(True) is True
        assert self.converter.serialize_langflow_object([1, 2, 3]) == [1, 2, 3]
        assert self.converter.serialize_langflow_object({"key": "value"}) == {
            "key": "value"
        }
        assert self.converter.serialize_langflow_object(None) is None

    def test_simple_basemodel_serialization(self):
        """Test serialization of simple BaseModel without special types."""
        model = SimpleTestModel(name="test", value=42, enabled=False)

        serialized = self.converter.serialize_langflow_object(model)

        # Check that it includes the data
        assert serialized["name"] == "test"
        assert serialized["value"] == 42
        assert serialized["enabled"] is False

        # Check that it includes class metadata
        assert serialized["__class_name__"] == "SimpleTestModel"
        assert serialized["__module_name__"] == "tests.unit.test_type_converter"

    def test_simple_basemodel_deserialization(self):
        """Test deserialization of simple BaseModel."""
        model = SimpleTestModel(name="test", value=42, enabled=False)
        serialized = self.converter.serialize_langflow_object(model)

        deserialized = self.converter.deserialize_to_langflow_type(serialized)

        # Check that it's properly reconstructed
        assert isinstance(deserialized, SimpleTestModel)
        assert deserialized.name == "test"
        assert deserialized.value == 42
        assert deserialized.enabled is False

    def test_secret_str_serialization_with_actual_secret(self):
        """Test that SecretStr fields are properly serialized with actual values."""
        secret_value = "sk-test-api-key-12345"
        model = SecretTestModel(
            name="test",
            api_key=SecretStr(secret_value),
            optional_secret=SecretStr("optional-secret-value"),
        )

        serialized = self.converter.serialize_langflow_object(model)

        # Check that secret values are properly extracted
        assert serialized["name"] == "test"
        assert serialized["api_key"] == secret_value
        assert serialized["optional_secret"] == "optional-secret-value"

        # Check class metadata
        assert serialized["__class_name__"] == "SecretTestModel"
        assert serialized["__module_name__"] == "tests.unit.test_type_converter"

    def test_secret_str_serialization_with_env_var_resolution(self):
        """Test that SecretStr fields with environment variable names are resolved."""
        # Set up environment variable
        test_api_key = "sk-actual-api-key-from-env"

        with patch.dict(os.environ, {"TEST_API_KEY": test_api_key}):
            # Create model with environment variable name as secret
            model = SecretTestModel(
                name="test",
                api_key=SecretStr("TEST_API_KEY"),  # Environment variable name
            )

            serialized = self.converter.serialize_langflow_object(model)

            # Check that environment variable was resolved
            assert serialized["api_key"] == test_api_key
            assert serialized["name"] == "test"

    def test_secret_str_serialization_no_env_var_fallback(self):
        """Test SecretStr serialization when environment variable doesn't exist."""
        # Create model with non-existent environment variable name
        model = SecretTestModel(name="test", api_key=SecretStr("NON_EXISTENT_ENV_VAR"))

        serialized = self.converter.serialize_langflow_object(model)

        # Should keep the original value if env var doesn't exist
        assert serialized["api_key"] == "NON_EXISTENT_ENV_VAR"
        assert serialized["name"] == "test"

    def test_openai_embeddings_like_serialization(self):
        """Test serialization of OpenAI-like embeddings model."""
        api_key = "sk-real-openai-key-12345"
        model = MockOpenAIEmbeddings(
            model="text-embedding-3-small",
            openai_api_key=SecretStr(api_key),
            chunk_size=500,
            dimensions=1536,
        )

        serialized = self.converter.serialize_langflow_object(model)

        # Check all fields are properly serialized
        assert serialized["model"] == "text-embedding-3-small"
        assert serialized["openai_api_key"] == api_key  # SecretStr should be unwrapped
        assert serialized["chunk_size"] == 500
        assert serialized["max_retries"] == 3  # Default value
        assert serialized["dimensions"] == 1536

        # Check class metadata for reconstruction
        assert serialized["__class_name__"] == "MockOpenAIEmbeddings"
        assert serialized["__module_name__"] == "tests.unit.test_type_converter"

    def test_openai_embeddings_like_deserialization(self):
        """Test deserialization of OpenAI-like embeddings model."""
        api_key = "sk-real-openai-key-12345"
        model = MockOpenAIEmbeddings(
            model="text-embedding-ada-002",
            openai_api_key=SecretStr(api_key),
            chunk_size=750,
        )

        # Serialize then deserialize
        serialized = self.converter.serialize_langflow_object(model)
        deserialized = self.converter.deserialize_to_langflow_type(serialized)

        # Check proper reconstruction
        assert isinstance(deserialized, MockOpenAIEmbeddings)
        assert deserialized.model == "text-embedding-ada-002"
        assert deserialized.chunk_size == 750
        assert deserialized.max_retries == 3  # Default

        # Important: Check that SecretStr field is reconstructed properly
        # After deserialization, it becomes a SecretStr again (correct Pydantic
        # behavior)
        assert isinstance(deserialized.openai_api_key, SecretStr)
        assert deserialized.openai_api_key.get_secret_value() == api_key

    def test_openai_api_key_env_var_resolution(self):
        """Test that OPENAI_API_KEY environment variable is properly resolved."""
        real_api_key = "sk-proj-real-openai-key-from-environment"

        with patch.dict(os.environ, {"OPENAI_API_KEY": real_api_key}):
            # Create model with environment variable name
            model = MockOpenAIEmbeddings(openai_api_key=SecretStr("OPENAI_API_KEY"))

            serialized = self.converter.serialize_langflow_object(model)

            # Verify the actual API key value is serialized, not the env var name
            assert serialized["openai_api_key"] == real_api_key

            # Test full round-trip
            deserialized = self.converter.deserialize_to_langflow_type(serialized)
            assert isinstance(deserialized.openai_api_key, SecretStr)
            assert deserialized.openai_api_key.get_secret_value() == real_api_key

    def test_mixed_secret_and_regular_fields(self):
        """Test model with both secret and regular fields."""
        api_key = "secret-key-value"
        model = SecretTestModel(
            name="production-model",
            api_key=SecretStr(api_key),
            optional_secret=None,  # Test None value
        )

        serialized = self.converter.serialize_langflow_object(model)

        # Regular field should be unchanged
        assert serialized["name"] == "production-model"
        # Secret field should be unwrapped
        assert serialized["api_key"] == api_key
        # None secret field should remain None
        assert serialized["optional_secret"] is None

    def test_non_basemodel_object_error(self):
        """Test that non-serializable objects raise appropriate errors."""

        class NonSerializableClass:
            def __init__(self):
                self.value = "test"

        obj = NonSerializableClass()

        with pytest.raises(ValueError, match="Cannot serialize object of type"):
            self.converter.serialize_langflow_object(obj)

    def test_deserialization_without_class_metadata(self):
        """Test deserialization of regular dict without class metadata."""
        regular_dict = {"name": "test", "value": 42}

        # Should return unchanged
        result = self.converter.deserialize_to_langflow_type(regular_dict)
        assert result == regular_dict

    def test_deserialization_with_invalid_class_metadata(self):
        """Test deserialization gracefully handles invalid class metadata."""
        invalid_serialized = {
            "name": "test",
            "__class_name__": "NonExistentClass",
            "__module_name__": "non.existent.module",
        }

        # Should return the dict unchanged if class can't be imported
        result = self.converter.deserialize_to_langflow_type(invalid_serialized)
        assert result == invalid_serialized

    def test_is_secret_str_type_detection(self):
        """Test the _is_secret_str_type method works correctly."""
        # Test direct SecretStr detection
        assert self.converter._is_secret_str_type(SecretStr)

        # Test Optional[SecretStr] detection
        assert self.converter._is_secret_str_type(SecretStr | None)

        # Test non-secret types
        assert not self.converter._is_secret_str_type(str)
        assert not self.converter._is_secret_str_type(int)
        assert not self.converter._is_secret_str_type(str | None)

    def test_langflow_types_still_work(self):
        """Test that existing Langflow type serialization still works."""
        try:
            from langflow.schema.message import Message

            message = Message(text="test message")
            serialized = self.converter.serialize_langflow_object(message)

            # Should have Langflow type metadata, not BaseModel metadata
            assert "__langflow_type__" in serialized
            assert serialized["__langflow_type__"] == "Message"

        except ImportError:
            # Skip if Langflow not available
            pytest.skip("Langflow not available for testing")

    @patch.dict(os.environ, {}, clear=True)  # Clear environment
    def test_secret_str_with_no_env_vars(self):
        """Test SecretStr serialization when no environment variables are set."""
        model = SecretTestModel(name="test", api_key=SecretStr("MISSING_API_KEY"))

        serialized = self.converter.serialize_langflow_object(model)

        # Should keep original value if env var resolution fails
        assert serialized["api_key"] == "MISSING_API_KEY"

    def test_serialized_data_structure_debugging(self):
        """Test to show what serialized data looks like for debugging."""
        api_key = "sk-debug-key-12345"
        model = MockOpenAIEmbeddings(
            model="test-model", openai_api_key=SecretStr(api_key), chunk_size=123
        )

        serialized = self.converter.serialize_langflow_object(model)

        # Print structure for debugging
        print(f"\nSerialized data structure: {serialized}")

        # Verify the structure
        assert "openai_api_key" in serialized
        assert (
            serialized["openai_api_key"] == api_key
        )  # Should be actual key, not hidden
        assert "__class_name__" in serialized
        assert "__module_name__" in serialized

        # Ensure we can deserialize
        deserialized = self.converter.deserialize_to_langflow_type(serialized)
        assert isinstance(deserialized, MockOpenAIEmbeddings)
        assert deserialized.openai_api_key.get_secret_value() == api_key

    def test_real_openai_embeddings_serialization(self):
        """Test with actual OpenAI embeddings class if available."""
        try:
            from langchain_openai import OpenAIEmbeddings
            from pydantic import SecretStr

            # Test with a real OpenAIEmbeddings instance
            api_key = "sk-test-real-openai-key"
            embeddings = OpenAIEmbeddings(
                model="text-embedding-3-small",
                openai_api_key=api_key,  # This becomes SecretStr automatically
                chunk_size=500,
            )

            # Serialize
            serialized = self.converter.serialize_langflow_object(embeddings)

            # Check that API key is properly extracted
            assert "openai_api_key" in serialized
            assert serialized["openai_api_key"] == api_key  # Should be actual key value
            assert serialized["model"] == "text-embedding-3-small"
            assert serialized["chunk_size"] == 500

            # Check metadata for reconstruction
            assert serialized["__class_name__"] == "OpenAIEmbeddings"
            assert "langchain_openai" in serialized["__module_name__"]

            # Test deserialization
            deserialized = self.converter.deserialize_to_langflow_type(serialized)
            assert isinstance(deserialized, OpenAIEmbeddings)
            assert deserialized.model == "text-embedding-3-small"
            assert deserialized.chunk_size == 500

            # Check that SecretStr is properly reconstructed
            assert hasattr(deserialized, "openai_api_key")
            if isinstance(deserialized.openai_api_key, SecretStr):
                assert deserialized.openai_api_key.get_secret_value() == api_key
            else:
                # In some versions, it might be a string after deserialization
                assert deserialized.openai_api_key == api_key

            print(
                "\n✅ Real OpenAI embeddings test passed with API key properly handled"
            )

        except ImportError:
            pytest.skip(
                "langchain_openai not available for real OpenAI embeddings test"
            )
