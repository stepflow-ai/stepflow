from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.diagnostic_message_type_6_type import DiagnosticMessageType6Type

T = TypeVar("T", bound="DiagnosticMessageType6")


@_attrs_define
class DiagnosticMessageType6:
    """
    Attributes:
        field (str):
        reason (str):
        step_id (str):
        type_ (DiagnosticMessageType6Type):
    """

    field: str
    reason: str
    step_id: str
    type_: DiagnosticMessageType6Type
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        field = self.field

        reason = self.reason

        step_id = self.step_id

        type_ = self.type_.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "field": field,
                "reason": reason,
                "stepId": step_id,
                "type": type_,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        field = d.pop("field")

        reason = d.pop("reason")

        step_id = d.pop("stepId")

        type_ = DiagnosticMessageType6Type(d.pop("type"))

        diagnostic_message_type_6 = cls(
            field=field,
            reason=reason,
            step_id=step_id,
            type_=type_,
        )

        diagnostic_message_type_6.additional_properties = d
        return diagnostic_message_type_6

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
