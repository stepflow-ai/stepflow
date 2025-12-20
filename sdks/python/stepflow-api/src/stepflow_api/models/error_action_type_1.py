from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.error_action_type_1_action import ErrorActionType1Action

T = TypeVar("T", bound="ErrorActionType1")


@_attrs_define
class ErrorActionType1:
    """# OnErrorSkip
    If the step fails, mark it as skipped. This allows down-stream steps to handle the skipped step.

        Attributes:
            action (ErrorActionType1Action):
    """

    action: ErrorActionType1Action
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        action = self.action.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "action": action,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        action = ErrorActionType1Action(d.pop("action"))

        error_action_type_1 = cls(
            action=action,
        )

        error_action_type_1.additional_properties = d
        return error_action_type_1

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
