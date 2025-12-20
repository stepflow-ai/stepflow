from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="DebugRunnableResponse")


@_attrs_define
class DebugRunnableResponse:
    """Response for runnable steps in debug mode

    Attributes:
        runnable_steps (list[str]): Steps that can be executed
    """

    runnable_steps: list[str]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        runnable_steps = self.runnable_steps

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "runnableSteps": runnable_steps,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        runnable_steps = cast(list[str], d.pop("runnableSteps"))

        debug_runnable_response = cls(
            runnable_steps=runnable_steps,
        )

        debug_runnable_response.additional_properties = d
        return debug_runnable_response

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
