from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.diagnostic_message_type_5_type import DiagnosticMessageType5Type
from ..types import UNSET, Unset

T = TypeVar("T", bound="DiagnosticMessageType5")


@_attrs_define
class DiagnosticMessageType5:
    """
    Attributes:
        error (str):
        type_ (DiagnosticMessageType5Type):
        field (None | str | Unset):
        step_id (None | str | Unset):
    """

    error: str
    type_: DiagnosticMessageType5Type
    field: None | str | Unset = UNSET
    step_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        error = self.error

        type_ = self.type_.value

        field: None | str | Unset
        if isinstance(self.field, Unset):
            field = UNSET
        else:
            field = self.field

        step_id: None | str | Unset
        if isinstance(self.step_id, Unset):
            step_id = UNSET
        else:
            step_id = self.step_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "error": error,
                "type": type_,
            }
        )
        if field is not UNSET:
            field_dict["field"] = field
        if step_id is not UNSET:
            field_dict["stepId"] = step_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        error = d.pop("error")

        type_ = DiagnosticMessageType5Type(d.pop("type"))

        def _parse_field(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        field = _parse_field(d.pop("field", UNSET))

        def _parse_step_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        step_id = _parse_step_id(d.pop("stepId", UNSET))

        diagnostic_message_type_5 = cls(
            error=error,
            type_=type_,
            field=field,
            step_id=step_id,
        )

        diagnostic_message_type_5.additional_properties = d
        return diagnostic_message_type_5

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
