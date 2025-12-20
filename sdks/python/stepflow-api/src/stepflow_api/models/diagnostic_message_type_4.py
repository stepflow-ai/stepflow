from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.diagnostic_message_type_4_type import DiagnosticMessageType4Type
from ..types import UNSET, Unset

T = TypeVar("T", bound="DiagnosticMessageType4")


@_attrs_define
class DiagnosticMessageType4:
    """
    Attributes:
        referenced_step (str):
        type_ (DiagnosticMessageType4Type):
        from_step (None | str | Unset):
    """

    referenced_step: str
    type_: DiagnosticMessageType4Type
    from_step: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        referenced_step = self.referenced_step

        type_ = self.type_.value

        from_step: None | str | Unset
        if isinstance(self.from_step, Unset):
            from_step = UNSET
        else:
            from_step = self.from_step

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "referencedStep": referenced_step,
                "type": type_,
            }
        )
        if from_step is not UNSET:
            field_dict["fromStep"] = from_step

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        referenced_step = d.pop("referencedStep")

        type_ = DiagnosticMessageType4Type(d.pop("type"))

        def _parse_from_step(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        from_step = _parse_from_step(d.pop("fromStep", UNSET))

        diagnostic_message_type_4 = cls(
            referenced_step=referenced_step,
            type_=type_,
            from_step=from_step,
        )

        diagnostic_message_type_4.additional_properties = d
        return diagnostic_message_type_4

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
