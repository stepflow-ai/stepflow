from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field
from dateutil.parser import isoparse

from ..models.execution_status import ExecutionStatus
from ..types import UNSET, Unset

T = TypeVar("T", bound="BatchRunInfo")


@_attrs_define
class BatchRunInfo:
    """Run information with batch context

    Attributes:
        created_at (datetime.datetime):
        debug_mode (bool):
        flow_id (str): A type-safe wrapper for blob identifiers.

            Blob IDs are SHA-256 hashes of the content, providing deterministic
            identification and automatic deduplication.
        run_id (UUID):
        status (ExecutionStatus): Status of a workflow execution
        batch_input_index (int): Position in the batch input array
        completed_at (datetime.datetime | None | Unset):
        flow_label (None | str | Unset):
        flow_name (None | str | Unset):
    """

    created_at: datetime.datetime
    debug_mode: bool
    flow_id: str
    run_id: UUID
    status: ExecutionStatus
    batch_input_index: int
    completed_at: datetime.datetime | None | Unset = UNSET
    flow_label: None | str | Unset = UNSET
    flow_name: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at.isoformat()

        debug_mode = self.debug_mode

        flow_id = self.flow_id

        run_id = str(self.run_id)

        status = self.status.value

        batch_input_index = self.batch_input_index

        completed_at: None | str | Unset
        if isinstance(self.completed_at, Unset):
            completed_at = UNSET
        elif isinstance(self.completed_at, datetime.datetime):
            completed_at = self.completed_at.isoformat()
        else:
            completed_at = self.completed_at

        flow_label: None | str | Unset
        if isinstance(self.flow_label, Unset):
            flow_label = UNSET
        else:
            flow_label = self.flow_label

        flow_name: None | str | Unset
        if isinstance(self.flow_name, Unset):
            flow_name = UNSET
        else:
            flow_name = self.flow_name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "createdAt": created_at,
                "debugMode": debug_mode,
                "flowId": flow_id,
                "runId": run_id,
                "status": status,
                "batchInputIndex": batch_input_index,
            }
        )
        if completed_at is not UNSET:
            field_dict["completedAt"] = completed_at
        if flow_label is not UNSET:
            field_dict["flowLabel"] = flow_label
        if flow_name is not UNSET:
            field_dict["flowName"] = flow_name

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = isoparse(d.pop("createdAt"))

        debug_mode = d.pop("debugMode")

        flow_id = d.pop("flowId")

        run_id = UUID(d.pop("runId"))

        status = ExecutionStatus(d.pop("status"))

        batch_input_index = d.pop("batchInputIndex")

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

        def _parse_flow_label(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        flow_label = _parse_flow_label(d.pop("flowLabel", UNSET))

        def _parse_flow_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        flow_name = _parse_flow_name(d.pop("flowName", UNSET))

        batch_run_info = cls(
            created_at=created_at,
            debug_mode=debug_mode,
            flow_id=flow_id,
            run_id=run_id,
            status=status,
            batch_input_index=batch_input_index,
            completed_at=completed_at,
            flow_label=flow_label,
            flow_name=flow_name,
        )

        batch_run_info.additional_properties = d
        return batch_run_info

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
