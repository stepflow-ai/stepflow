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

"""Field handler that converts Data lists to DataFrames."""

from __future__ import annotations

import logging
from typing import Any

from .base import FieldHandler

logger = logging.getLogger(__name__)


def _is_data_list(value: list[Any]) -> bool:
    """Check if a list contains Data-like objects suitable for DataFrame conversion."""
    return all(
        (isinstance(item, dict) and ("text" in item or "__class_name__" in item))
        or (hasattr(item, "__class__") and item.__class__.__name__ == "Data")
        for item in value
        if item is not None
    )


class DataFrameFieldHandler(FieldHandler):
    """Convert lists of Data objects to DataFrames for DataFrame-typed fields.

    Matches template fields whose ``input_types`` include ``"DataFrame"``.
    When the runtime value is a non-empty list of Data-like objects (dicts
    with ``text``/``__class_name__`` keys, or ``Data`` instances), converts
    it to an ``lfx.schema.dataframe.DataFrame``.
    """

    def matches(self, template_field: dict[str, Any]) -> bool:
        return "DataFrame" in template_field.get("input_types", [])

    async def prepare(
        self, fields: dict[str, tuple[Any, dict[str, Any]]], context: Any
    ) -> dict[str, Any]:
        result: dict[str, Any] = {}

        for key, (value, _template_field) in fields.items():
            if not isinstance(value, list) or not value:
                continue

            if not _is_data_list(value):
                continue

            try:
                from lfx.schema.dataframe import DataFrame

                result[key] = DataFrame(data=value)
            except Exception:
                # If DataFrame conversion fails, keep as list
                logger.debug("DataFrame conversion failed for field %r", key)

        return result
