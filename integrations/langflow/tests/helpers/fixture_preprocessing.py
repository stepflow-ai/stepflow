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

"""Flow fixture preprocessing utilities.

This module provides utilities for preprocessing Langflow JSON fixtures before
conversion to Stepflow workflows. This replaces runtime environment variable
resolution in the UDF executor with test-time preprocessing.

Key benefits:
- Separates test concerns from production component execution logic
- Allows tests to override specific fixture fields without hardcoded fallbacks
- Makes fixture parameterization explicit and controllable
- Removes complex runtime environment variable resolution
"""

import json
import os
from typing import Any


class FlowFixturePreprocessor:
    """Preprocess Langflow fixtures with environment variable field overrides."""

    def __init__(self, environment: dict[str, str] | None = None):
        """Initialize preprocessor with optional environment override.

        Args:
            environment: Custom environment dict (defaults to os.environ)
        """
        self.environment = environment or dict(os.environ)

    def preprocess_fixture(
        self,
        fixture_data: dict[str, Any],
        field_overrides: dict[str, dict[str, Any]] | None = None,
    ) -> dict[str, Any]:
        """Preprocess a fixture with environment variable field overrides.

        Args:
            fixture_data: Original Langflow JSON fixture data
            field_overrides: Dict mapping node_id -> {field_name: new_value}

        Returns:
            Preprocessed fixture data with substitutions applied
        """
        # Deep copy to avoid mutating original
        processed = json.loads(json.dumps(fixture_data))
        field_overrides = field_overrides or {}

        # Process each node
        for node in processed.get("data", {}).get("nodes", []):
            node_id = node.get("id")
            node_data = node.get("data", {})
            template = node_data.get("node", {}).get("template", {})

            # Apply field overrides first
            if node_id in field_overrides:
                for field_name, new_value in field_overrides[node_id].items():
                    if field_name in template and isinstance(
                        template[field_name], dict
                    ):
                        template[field_name]["value"] = new_value

            # Apply environment variable substitution
            for _field_name, field_config in template.items():
                if isinstance(field_config, dict) and "value" in field_config:
                    field_config["value"] = self._resolve_field_value(
                        field_config["value"], field_config
                    )

        return processed

    def _resolve_field_value(self, value: Any, field_config: dict[str, Any]) -> Any:
        """Resolve a single field value with environment variable substitution.

        Args:
            value: Original field value
            field_config: Field configuration with type info

        Returns:
            Resolved field value
        """
        if not isinstance(value, str):
            return value

        # Handle ${ENV_VAR} template format
        if value.startswith("${") and value.endswith("}"):
            env_var = value[2:-1]
            return self.environment.get(env_var, value)  # Keep original if not found

        # Handle direct environment variable names
        if self._is_env_var_reference(value, field_config):
            return self.environment.get(value, value)  # Keep original if not found

        return value

    def _is_env_var_reference(self, value: str, field_config: dict[str, Any]) -> bool:
        """Check if a string value is an environment variable reference.

        Args:
            value: String value to check
            field_config: Field configuration

        Returns:
            True if this appears to be an env var reference
        """
        # Common API key patterns
        if value in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "ASTRA_DB_APPLICATION_TOKEN",
            "ASTRA_DB_API_ENDPOINT",
        ]:
            return True

        # SecretStrInput fields with uppercase underscore pattern
        if (
            field_config.get("_input_type") == "SecretStrInput"
            and value.isupper()
            and "_" in value
        ):
            return True

        return False


def load_and_preprocess_fixture(
    fixture_name: str,
    field_overrides: dict[str, dict[str, Any]] | None = None,
    environment: dict[str, str] | None = None,
) -> dict[str, Any]:
    """Load and preprocess a Langflow fixture file.

    Args:
        fixture_name: Name of fixture file (without .json extension)
        field_overrides: Dict mapping node_id -> {field_name: new_value}
        environment: Custom environment dict (defaults to os.environ)

    Returns:
        Preprocessed fixture data
    """
    from pathlib import Path

    fixture_path = (
        Path(__file__).parent.parent / "fixtures" / "langflow" / f"{fixture_name}.json"
    )

    with open(fixture_path) as f:
        fixture_data = json.load(f)

    preprocessor = FlowFixturePreprocessor(environment)
    return preprocessor.preprocess_fixture(fixture_data, field_overrides)


# Convenience functions for common test scenarios
def preprocess_openai_fixture(
    fixture_name: str, api_key: str | None = None
) -> dict[str, Any]:
    """Preprocess fixture with OpenAI API key substitution.

    Args:
        fixture_name: Name of fixture file
        api_key: OpenAI API key (defaults to environment)

    Returns:
        Preprocessed fixture with OpenAI API key resolved
    """
    env = {}
    if api_key:
        env["OPENAI_API_KEY"] = api_key

    return load_and_preprocess_fixture(fixture_name, environment=env)


def preprocess_astradb_fixture(
    fixture_name: str,
    api_endpoint: str | None = None,
    application_token: str | None = None,
    database_name: str = "langflow-test",
    collection_name: str = "test_collection",
) -> dict[str, Any]:
    """Preprocess fixture with AstraDB configuration.

    Args:
        fixture_name: Name of fixture file
        api_endpoint: AstraDB API endpoint
        application_token: AstraDB application token
        database_name: Database name override
        collection_name: Collection name override

    Returns:
        Preprocessed fixture with AstraDB configuration
    """
    env = {}
    if api_endpoint:
        env["ASTRA_DB_API_ENDPOINT"] = api_endpoint
    if application_token:
        env["ASTRA_DB_APPLICATION_TOKEN"] = application_token

    # Find AstraDB nodes and override database/collection names
    field_overrides = {}

    # This is a simplified approach - in practice, you'd need to identify
    # specific AstraDB node IDs in each fixture
    # field_overrides["AstraDB-node-id"] = {
    #     "database_name": database_name,
    #     "collection_name": collection_name
    # }

    return load_and_preprocess_fixture(fixture_name, field_overrides, env)


def substitute_component_inputs(
    langflow_data: dict[str, Any],
    component_id_prefix: str,
    input_substitutions: dict[str, Any],
) -> dict[str, Any]:
    """Substitute input values in components matching a given ID prefix.

    Args:
        langflow_data: Raw langflow fixture data
        component_id_prefix: Prefix to match component IDs (e.g., "AstraDB", "OpenAI")
        input_substitutions: Dict mapping input names to new values

    Returns:
        Updated langflow data with substitutions applied
    """
    import copy

    # Make a deep copy to avoid modifying the original
    updated_data = copy.deepcopy(langflow_data)

    if "data" not in updated_data or "nodes" not in updated_data["data"]:
        return updated_data

    # Find nodes matching the component prefix and apply substitutions
    for node in updated_data["data"]["nodes"]:
        node_id = node.get("id", "")
        if node_id.startswith(component_id_prefix):
            template = node.get("data", {}).get("node", {}).get("template", {})

            # Apply each input substitution
            for input_name, new_value in input_substitutions.items():
                if input_name in template:
                    # Update the value field of the input
                    template[input_name]["value"] = new_value

    return updated_data


def substitute_astradb_inputs(langflow_data: dict[str, Any]) -> dict[str, Any]:
    """Apply AstraDB-specific substitutions to components with AstraDB prefix.

    Args:
        langflow_data: Raw langflow fixture data

    Returns:
        Updated langflow data with AstraDB substitutions applied
    """
    import os

    # Load environment variables from .env if available
    env_vars = {}
    try:
        from dotenv import load_dotenv

        load_dotenv()
        env_vars = dict(os.environ)
    except ImportError:
        env_vars = dict(os.environ)

    # Get required AstraDB environment variables
    astra_token = env_vars.get("ASTRA_DB_APPLICATION_TOKEN")
    astra_endpoint = env_vars.get("ASTRA_DB_API_ENDPOINT")

    if not astra_token:
        import pytest

        pytest.skip("ASTRA_DB_APPLICATION_TOKEN environment variable is required")

    if not astra_endpoint:
        import pytest

        pytest.skip("ASTRA_DB_API_ENDPOINT environment variable is required")

    # Define AstraDB-specific input substitutions
    astradb_substitutions = {
        "token": astra_token,
        "api_endpoint": astra_endpoint,
        "database_name": "langflow-test",
        "collection_name": "test_collection",
    }

    return substitute_component_inputs(langflow_data, "AstraDB", astradb_substitutions)


def substitute_openai_inputs(langflow_data: dict[str, Any]) -> dict[str, Any]:
    """Apply OpenAI-specific substitutions to components that use OpenAI API keys.

    Args:
        langflow_data: Raw langflow fixture data

    Returns:
        Updated langflow data with OpenAI substitutions applied
    """
    import copy
    import os

    # Load environment variables from .env if available
    env_vars = {}
    try:
        from dotenv import load_dotenv

        load_dotenv()
        env_vars = dict(os.environ)
    except ImportError:
        env_vars = dict(os.environ)

    # Get OpenAI API key
    openai_key = env_vars.get("OPENAI_API_KEY")

    if not openai_key:
        import pytest

        pytest.skip("OPENAI_API_KEY environment variable is required")

    # Make a deep copy to avoid modifying the original
    updated_data = copy.deepcopy(langflow_data)

    if "data" not in updated_data or "nodes" not in updated_data["data"]:
        return updated_data

    # Find nodes that have OpenAI-related fields and apply substitutions
    for node in updated_data["data"]["nodes"]:
        template = node.get("data", {}).get("node", {}).get("template", {})

        # Check if any field references OPENAI_API_KEY or has openai_api_key field
        for field_name, field_def in template.items():
            if isinstance(field_def, dict) and "value" in field_def:
                value = field_def["value"]

                # Substitute if field uses OPENAI_API_KEY reference or
                # is an openai_api_key field
                if value == "OPENAI_API_KEY" or field_name in [
                    "openai_api_key",
                    "api_key",
                ]:
                    field_def["value"] = openai_key

    return updated_data


def substitute_astradb_and_openai_inputs(
    langflow_data: dict[str, Any],
) -> dict[str, Any]:
    """Apply both AstraDB and OpenAI substitutions to a flow.

    Args:
        langflow_data: Raw langflow fixture data

    Returns:
        Updated langflow data with both AstraDB and OpenAI substitutions applied
    """
    # Apply AstraDB substitutions first
    updated_data = substitute_astradb_inputs(langflow_data)

    # Then apply OpenAI substitutions
    updated_data = substitute_openai_inputs(updated_data)

    return updated_data
