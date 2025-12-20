from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="DependencyType1StepOutput")


@_attrs_define
class DependencyType1StepOutput:
    """Comes from another step's output

    Attributes:
        optional (bool): If true, the step_id may be skipped and this step still executed.
        step_id (str): Which step produces this data
        field (None | str | Unset): Optional field path within step output
    """

    optional: bool
    step_id: str
    field: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        optional = self.optional

        step_id = self.step_id

        field: None | str | Unset
        if isinstance(self.field, Unset):
            field = UNSET
        else:
            field = self.field

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "optional": optional,
                "stepId": step_id,
            }
        )
        if field is not UNSET:
            field_dict["field"] = field

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        optional = d.pop("optional")

        step_id = d.pop("stepId")

        def _parse_field(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        field = _parse_field(d.pop("field", UNSET))

        dependency_type_1_step_output = cls(
            optional=optional,
            step_id=step_id,
            field=field,
        )

        dependency_type_1_step_output.additional_properties = d
        return dependency_type_1_step_output

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
