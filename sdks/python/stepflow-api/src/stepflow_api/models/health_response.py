from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="HealthResponse")


@_attrs_define
class HealthResponse:
    """Health check response

    Attributes:
        status (str): Service status
        timestamp (str): Timestamp when health was checked (RFC3339 format)
        version (str): Service version
    """

    status: str
    timestamp: str
    version: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        status = self.status

        timestamp = self.timestamp

        version = self.version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "status": status,
                "timestamp": timestamp,
                "version": version,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        status = d.pop("status")

        timestamp = d.pop("timestamp")

        version = d.pop("version")

        health_response = cls(
            status=status,
            timestamp=timestamp,
            version=version,
        )

        health_response.additional_properties = d
        return health_response

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
