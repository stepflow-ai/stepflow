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

"""Field handler that resolves environment variables for ``load_from_db`` fields."""

from __future__ import annotations

import os
from typing import Any

from ...exceptions import ExecutionError
from .base import FieldHandler


class EnvVarFieldHandler(FieldHandler):
    """Resolve empty parameter values from environment variables.

    Matches template fields with ``load_from_db=True``. For matched fields
    whose runtime value is falsy, the handler reads the environment variable
    named in the template field's ``value`` key.
    """

    def matches(self, template_field: dict[str, Any]) -> bool:
        return template_field.get("load_from_db", False) is True

    async def prepare(
        self, fields: dict[str, tuple[Any, dict[str, Any]]], context: Any
    ) -> dict[str, Any]:
        result: dict[str, Any] = {}

        for key, (value, template_field) in fields.items():
            # Skip if value is already set (truthy)
            if value:
                continue

            # The variable name comes from the template field's "value" key
            var_name = template_field.get("value", key)
            if not var_name:
                continue

            env_value = os.environ.get(var_name)
            if env_value is None:
                raise ExecutionError(
                    f"Environment variable '{var_name}' for parameter '{key}' "
                    f"not set. Either provide the value as a runtime variable "
                    f"or set the environment variable."
                )
            result[key] = env_value

        return result
