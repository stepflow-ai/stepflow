from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.diagnostic_message_type_2_type import DiagnosticMessageType2Type

T = TypeVar("T", bound="DiagnosticMessageType2")


@_attrs_define
class DiagnosticMessageType2:
    """
    Attributes:
        from_step (str):
        to_step (str):
        type_ (DiagnosticMessageType2Type):
    """

    from_step: str
    to_step: str
    type_: DiagnosticMessageType2Type
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from_step = self.from_step

        to_step = self.to_step

        type_ = self.type_.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "fromStep": from_step,
                "toStep": to_step,
                "type": type_,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        from_step = d.pop("fromStep")

        to_step = d.pop("toStep")

        type_ = DiagnosticMessageType2Type(d.pop("type"))

        diagnostic_message_type_2 = cls(
            from_step=from_step,
            to_step=to_step,
            type_=type_,
        )

        diagnostic_message_type_2.additional_properties = d
        return diagnostic_message_type_2

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
