"""Common types for the API client."""

from __future__ import annotations

from dataclasses import dataclass
from http import HTTPStatus
from typing import Any, Generic, TypeVar


# Sentinel for unset values (compatible with pydantic's exclude_unset)
class _Unset:
    def __bool__(self) -> bool:
        return False

    def __repr__(self) -> str:
        return "UNSET"


UNSET = _Unset()
UnsetType = _Unset
Unset = _Unset  # Alias for compatibility


T = TypeVar("T")


@dataclass
class Response(Generic[T]):
    """HTTP response wrapper."""

    status_code: HTTPStatus
    content: bytes
    headers: dict[str, Any]
    parsed: T | None = None
