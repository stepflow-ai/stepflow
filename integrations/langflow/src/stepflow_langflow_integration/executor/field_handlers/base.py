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

"""Base class for field handlers."""

from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any


class FieldHandler(ABC):
    """Abstract base class for template-field-based value transformations.

    Subclasses declare which template fields they handle via ``matches()``,
    optionally set up resources via ``activate()``, and transform values
    in ``prepare()``.
    """

    @abstractmethod
    def matches(self, template_field: dict[str, Any]) -> bool:
        """Return True if this handler should process the given template field."""
        ...

    @asynccontextmanager
    async def activate(self) -> AsyncIterator[Any]:
        """Set up and tear down handler resources.

        Only entered when at least one field matches. Yields a context value
        that is passed to ``prepare()``. The default implementation yields
        ``None`` (no resources needed).
        """
        yield None

    @abstractmethod
    async def prepare(
        self, fields: dict[str, tuple[Any, dict[str, Any]]], context: Any
    ) -> dict[str, Any]:
        """Batch-process all matched fields.

        Args:
            fields: Mapping of ``{key: (value, template_field)}`` for every
                parameter whose template field matched this handler.
            context: The value yielded by ``activate()``.

        Returns:
            Mapping of ``{key: resolved_value}`` for fields whose values
            changed. Fields omitted from the result keep their original value.
        """
        ...
