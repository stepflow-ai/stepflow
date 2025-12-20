from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.override_type import OverrideType
from ..types import UNSET, Unset

T = TypeVar("T", bound="StepOverride")


@_attrs_define
class StepOverride:
    """Override specification for a single step.

    Contains the override type (merge patch, json patch, etc.) and the value
    to apply. The type field uses `$type` to avoid collisions with step properties.

        Attributes:
            value (Any): The override value to apply, interpreted based on the override type.
            type_ (OverrideType | Unset): The type of override operation to perform.
    """

    value: Any
    type_: OverrideType | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        value = self.value

        type_: str | Unset = UNSET
        if not isinstance(self.type_, Unset):
            type_ = self.type_.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "value": value,
            }
        )
        if type_ is not UNSET:
            field_dict["$type"] = type_

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        value = d.pop("value")

        _type_ = d.pop("$type", UNSET)
        type_: OverrideType | Unset
        if isinstance(_type_, Unset):
            type_ = UNSET
        else:
            type_ = OverrideType(_type_)

        step_override = cls(
            value=value,
            type_=type_,
        )

        step_override.additional_properties = d
        return step_override

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
