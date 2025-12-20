from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.run_summary import RunSummary


T = TypeVar("T", bound="ListRunsResponse")


@_attrs_define
class ListRunsResponse:
    """Response for listing runs

    Attributes:
        runs (list[RunSummary]): List of run summaries
    """

    runs: list[RunSummary]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        runs = []
        for runs_item_data in self.runs:
            runs_item = runs_item_data.to_dict()
            runs.append(runs_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "runs": runs,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.run_summary import RunSummary

        d = dict(src_dict)
        runs = []
        _runs = d.pop("runs")
        for runs_item_data in _runs:
            runs_item = RunSummary.from_dict(runs_item_data)

            runs.append(runs_item)

        list_runs_response = cls(
            runs=runs,
        )

        list_runs_response.additional_properties = d
        return list_runs_response

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
