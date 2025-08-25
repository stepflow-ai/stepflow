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

"""Environment variable utilities."""

import os
from typing import Optional


def determine_environment_variable(
    field_name: str, field_value: str, field_config: dict
) -> Optional[str]:
    """Determine environment variable name from Langflow field metadata.

    Args:
        field_name: Name of the field
        field_value: Current field value
        field_config: Field configuration dict

    Returns:
        Environment variable name or None
    """
    # Case 1: Template string like "${OPENAI_API_KEY}"
    if (
        isinstance(field_value, str)
        and field_value.startswith("${")
        and field_value.endswith("}")
    ):
        return field_value[2:-1]  # Remove ${ and }

    # Case 2: Direct environment variable name
    if (
        isinstance(field_value, str)
        and field_value.isupper()
        and "_" in field_value
        and any(
            keyword in field_value
            for keyword in ["API_KEY", "TOKEN", "SECRET", "ENDPOINT"]
        )
    ):
        return field_value

    # Case 3: Secret input fields with smart mapping
    if field_config.get("_input_type") == "SecretStrInput":
        field_upper = field_name.upper()

        # Smart mapping for common patterns
        if field_name == "api_key":
            return "OPENAI_API_KEY"  # Most common default
        elif field_name == "openai_api_key":
            return "OPENAI_API_KEY"
        elif field_name == "token":
            return "ASTRA_DB_APPLICATION_TOKEN"
        elif "openai" in field_name.lower():
            return "OPENAI_API_KEY"
        elif "anthropic" in field_name.lower():
            return "ANTHROPIC_API_KEY"
        elif "google" in field_name.lower():
            return "GOOGLE_API_KEY"
        elif "astra" in field_name.lower():
            return "ASTRA_DB_APPLICATION_TOKEN"
        else:
            # Generic pattern: convert field name to env var format
            return field_upper

    return None


def resolve_environment_value(
    env_var: str, default: Optional[str] = None
) -> Optional[str]:
    """Resolve environment variable value.

    Args:
        env_var: Environment variable name
        default: Default value if env var not found

    Returns:
        Environment variable value or default
    """
    return os.getenv(env_var, default)
