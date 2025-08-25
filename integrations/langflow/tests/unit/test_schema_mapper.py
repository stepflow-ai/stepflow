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

"""Unit tests for SchemaMapper."""

import pytest
from stepflow_langflow_integration.converter.schema_mapper import SchemaMapper


class TestSchemaMapper:
    """Test SchemaMapper functionality."""

    def test_init(self):
        """Test initialization."""
        mapper = SchemaMapper()
        assert "str" in mapper.langflow_to_json_schema
        assert mapper.langflow_to_json_schema["str"] == "string"

    def test_extract_output_schema_from_outputs(self):
        """Test schema extraction from outputs metadata."""
        mapper = SchemaMapper()
        node = {
            "data": {
                "type": "ChatInput",
                "outputs": [{"name": "message", "types": ["Message"]}],
            }
        }

        schema = mapper.extract_output_schema(node)

        assert schema["type"] == "object"
        assert "text" in schema["properties"]
        assert schema["properties"]["type"]["const"] == "Message"

    def test_extract_output_schema_from_base_classes(self):
        """Test schema extraction from base classes."""
        mapper = SchemaMapper()
        node = {"data": {"type": "CustomComponent", "base_classes": ["Data"]}}

        schema = mapper.extract_output_schema(node)

        assert schema["type"] == "object"
        assert "data" in schema["properties"]
        assert schema["properties"]["type"]["const"] == "Data"

    def test_extract_output_schema_from_heuristics(self):
        """Test schema extraction using component heuristics."""
        mapper = SchemaMapper()
        node = {"data": {"type": "ChatInput"}}

        schema = mapper.extract_output_schema(node)

        assert schema["type"] == "object"
        assert "text" in schema["properties"]
        assert schema["properties"]["type"]["const"] == "Message"

    def test_extract_output_schema_fallback(self):
        """Test fallback schema for unknown components."""
        mapper = SchemaMapper()
        node = {"data": {"type": "UnknownComponent"}}

        schema = mapper.extract_output_schema(node)

        assert schema["type"] == "object"
        assert "result" in schema["properties"]

    def test_extract_input_schema_simple(self):
        """Test input schema extraction from template."""
        mapper = SchemaMapper()
        node = {
            "data": {
                "node": {
                    "template": {
                        "input_value": {
                            "type": "str",
                            "info": "Input message",
                            "required": True,
                        },
                        "temperature": {
                            "type": "float",
                            "info": "Temperature setting",
                            "required": False,
                        },
                    }
                }
            }
        }

        schema = mapper.extract_input_schema(node)

        assert schema["type"] == "object"
        assert len(schema["properties"]) == 2
        assert schema["properties"]["input_value"]["type"] == "string"
        assert schema["properties"]["temperature"]["type"] == "number"
        assert schema["required"] == ["input_value"]

    def test_extract_input_schema_dropdown(self):
        """Test input schema with dropdown field."""
        mapper = SchemaMapper()
        node = {
            "data": {
                "node": {
                    "template": {
                        "model": {
                            "type": "dropdown",
                            "options": ["gpt-3.5-turbo", "gpt-4"],
                            "info": "Model selection",
                        }
                    }
                }
            }
        }

        schema = mapper.extract_input_schema(node)

        assert schema["properties"]["model"]["type"] == "string"
        assert schema["properties"]["model"]["enum"] == ["gpt-3.5-turbo", "gpt-4"]
