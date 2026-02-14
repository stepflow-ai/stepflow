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

"""Field handler that coerces Langflow Message objects to plain strings."""

from __future__ import annotations

from typing import Any

from .base import FieldHandler


class StringCoercionFieldHandler(FieldHandler):
    """Coerce Langflow Message objects to their ``.text`` for string fields.

    Matches template fields with ``type == "str"``. When the runtime value is
    a Langflow ``Message`` instance, extracts its ``text`` attribute so the
    component receives a plain string as expected.

    This is needed because lfx components (1.6.4+) are stricter about type
    validation and reject Message objects for string-typed inputs.
    """

    def matches(self, template_field: dict[str, Any]) -> bool:
        return template_field.get("type") == "str"

    async def prepare(
        self, fields: dict[str, tuple[Any, dict[str, Any]]], context: Any
    ) -> dict[str, Any]:
        result: dict[str, Any] = {}

        for key, (value, _template_field) in fields.items():
            if (
                hasattr(value, "__class__")
                and value.__class__.__name__ == "Message"
                and hasattr(value, "text")
            ):
                result[key] = value.text

        return result
