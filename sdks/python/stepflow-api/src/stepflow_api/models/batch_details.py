from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field
from dateutil.parser import isoparse

from ..models.batch_status import BatchStatus
from ..types import UNSET, Unset

T = TypeVar("T", bound="BatchDetails")


@_attrs_define
class BatchDetails:
    """Complete batch details for API responses

    Attributes:
        batch_id (UUID):
        created_at (datetime.datetime):
        flow_id (str): A type-safe wrapper for blob identifiers.

            Blob IDs are SHA-256 hashes of the content, providing deterministic
            identification and automatic deduplication.
        status (BatchStatus): Batch execution status
        total_inputs (int):
        cancelled_runs (int):
        completed_runs (int):
        failed_runs (int):
        paused_runs (int):
        running_runs (int):
        flow_name (None | str | Unset):
        completed_at (datetime.datetime | None | Unset):
    """

    batch_id: UUID
    created_at: datetime.datetime
    flow_id: str
    status: BatchStatus
    total_inputs: int
    cancelled_runs: int
    completed_runs: int
    failed_runs: int
    paused_runs: int
    running_runs: int
    flow_name: None | str | Unset = UNSET
    completed_at: datetime.datetime | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        batch_id = str(self.batch_id)

        created_at = self.created_at.isoformat()

        flow_id = self.flow_id

        status = self.status.value

        total_inputs = self.total_inputs

        cancelled_runs = self.cancelled_runs

        completed_runs = self.completed_runs

        failed_runs = self.failed_runs

        paused_runs = self.paused_runs

        running_runs = self.running_runs

        flow_name: None | str | Unset
        if isinstance(self.flow_name, Unset):
            flow_name = UNSET
        else:
            flow_name = self.flow_name

        completed_at: None | str | Unset
        if isinstance(self.completed_at, Unset):
            completed_at = UNSET
        elif isinstance(self.completed_at, datetime.datetime):
            completed_at = self.completed_at.isoformat()
        else:
            completed_at = self.completed_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "batchId": batch_id,
                "createdAt": created_at,
                "flowId": flow_id,
                "status": status,
                "totalInputs": total_inputs,
                "cancelledRuns": cancelled_runs,
                "completedRuns": completed_runs,
                "failedRuns": failed_runs,
                "pausedRuns": paused_runs,
                "runningRuns": running_runs,
            }
        )
        if flow_name is not UNSET:
            field_dict["flowName"] = flow_name
        if completed_at is not UNSET:
            field_dict["completedAt"] = completed_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        batch_id = UUID(d.pop("batchId"))

        created_at = isoparse(d.pop("createdAt"))

        flow_id = d.pop("flowId")

        status = BatchStatus(d.pop("status"))

        total_inputs = d.pop("totalInputs")

        cancelled_runs = d.pop("cancelledRuns")

        completed_runs = d.pop("completedRuns")

        failed_runs = d.pop("failedRuns")

        paused_runs = d.pop("pausedRuns")

        running_runs = d.pop("runningRuns")

        def _parse_flow_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        flow_name = _parse_flow_name(d.pop("flowName", UNSET))

        def _parse_completed_at(data: object) -> datetime.datetime | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                completed_at_type_0 = isoparse(data)

                return completed_at_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(datetime.datetime | None | Unset, data)

        completed_at = _parse_completed_at(d.pop("completedAt", UNSET))

        batch_details = cls(
            batch_id=batch_id,
            created_at=created_at,
            flow_id=flow_id,
            status=status,
            total_inputs=total_inputs,
            cancelled_runs=cancelled_runs,
            completed_runs=completed_runs,
            failed_runs=failed_runs,
            paused_runs=paused_runs,
            running_runs=running_runs,
            flow_name=flow_name,
            completed_at=completed_at,
        )

        batch_details.additional_properties = d
        return batch_details

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
