from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.diagnostic_message_type_18_type import DiagnosticMessageType18Type

T = TypeVar("T", bound="DiagnosticMessageType18")


@_attrs_define
class DiagnosticMessageType18:
    """
    Attributes:
        plugin (str):
        type_ (DiagnosticMessageType18Type):
    """

    plugin: str
    type_: DiagnosticMessageType18Type
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        plugin = self.plugin

        type_ = self.type_.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "plugin": plugin,
                "type": type_,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        plugin = d.pop("plugin")

        type_ = DiagnosticMessageType18Type(d.pop("type"))

        diagnostic_message_type_18 = cls(
            plugin=plugin,
            type_=type_,
        )

        diagnostic_message_type_18.additional_properties = d
        return diagnostic_message_type_18

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
