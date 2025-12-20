from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.error_action_type_2_action import ErrorActionType2Action
from ..types import UNSET, Unset

T = TypeVar("T", bound="ErrorActionType2")


@_attrs_define
class ErrorActionType2:
    """# OnErrorDefault
    If the step fails, use the `defaultValue` instead.

        Attributes:
            action (ErrorActionType2Action):
            default_value (Any | None | Unset):
    """

    action: ErrorActionType2Action
    default_value: Any | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        action = self.action.value

        default_value: Any | None | Unset
        if isinstance(self.default_value, Unset):
            default_value = UNSET
        else:
            default_value = self.default_value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "action": action,
            }
        )
        if default_value is not UNSET:
            field_dict["defaultValue"] = default_value

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        action = ErrorActionType2Action(d.pop("action"))

        def _parse_default_value(data: object) -> Any | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Any | None | Unset, data)

        default_value = _parse_default_value(d.pop("defaultValue", UNSET))

        error_action_type_2 = cls(
            action=action,
            default_value=default_value,
        )

        error_action_type_2.additional_properties = d
        return error_action_type_2

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
