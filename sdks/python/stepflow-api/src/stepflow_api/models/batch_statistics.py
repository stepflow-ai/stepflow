from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="BatchStatistics")


@_attrs_define
class BatchStatistics:
    """Calculated batch statistics (not stored, computed via queries)

    Attributes:
        cancelled_runs (int):
        completed_runs (int):
        failed_runs (int):
        paused_runs (int):
        running_runs (int):
    """

    cancelled_runs: int
    completed_runs: int
    failed_runs: int
    paused_runs: int
    running_runs: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        cancelled_runs = self.cancelled_runs

        completed_runs = self.completed_runs

        failed_runs = self.failed_runs

        paused_runs = self.paused_runs

        running_runs = self.running_runs

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "cancelledRuns": cancelled_runs,
                "completedRuns": completed_runs,
                "failedRuns": failed_runs,
                "pausedRuns": paused_runs,
                "runningRuns": running_runs,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        cancelled_runs = d.pop("cancelledRuns")

        completed_runs = d.pop("completedRuns")

        failed_runs = d.pop("failedRuns")

        paused_runs = d.pop("pausedRuns")

        running_runs = d.pop("runningRuns")

        batch_statistics = cls(
            cancelled_runs=cancelled_runs,
            completed_runs=completed_runs,
            failed_runs=failed_runs,
            paused_runs=paused_runs,
            running_runs=running_runs,
        )

        batch_statistics.additional_properties = d
        return batch_statistics

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
