from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.list_step_runs_response_steps import ListStepRunsResponseSteps


T = TypeVar("T", bound="ListStepRunsResponse")


@_attrs_define
class ListStepRunsResponse:
    """Response for listing step runs

    Attributes:
        steps (ListStepRunsResponseSteps): Dictionary of step run results keyed by step ID
    """

    steps: ListStepRunsResponseSteps
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        steps = self.steps.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "steps": steps,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.list_step_runs_response_steps import ListStepRunsResponseSteps

        d = dict(src_dict)
        steps = ListStepRunsResponseSteps.from_dict(d.pop("steps"))

        list_step_runs_response = cls(
            steps=steps,
        )

        list_step_runs_response.additional_properties = d
        return list_step_runs_response

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
