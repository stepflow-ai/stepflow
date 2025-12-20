from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.diagnostic_message_type_17_type import DiagnosticMessageType17Type

T = TypeVar("T", bound="DiagnosticMessageType17")


@_attrs_define
class DiagnosticMessageType17:
    """
    Attributes:
        plugin (str):
        route_path (str):
        rule_index (int):
        type_ (DiagnosticMessageType17Type):
    """

    plugin: str
    route_path: str
    rule_index: int
    type_: DiagnosticMessageType17Type
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        plugin = self.plugin

        route_path = self.route_path

        rule_index = self.rule_index

        type_ = self.type_.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "plugin": plugin,
                "routePath": route_path,
                "ruleIndex": rule_index,
                "type": type_,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        plugin = d.pop("plugin")

        route_path = d.pop("routePath")

        rule_index = d.pop("ruleIndex")

        type_ = DiagnosticMessageType17Type(d.pop("type"))

        diagnostic_message_type_17 = cls(
            plugin=plugin,
            route_path=route_path,
            rule_index=rule_index,
            type_=type_,
        )

        diagnostic_message_type_17.additional_properties = d
        return diagnostic_message_type_17

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
