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

T = TypeVar("T", bound="BatchMetadata")


@_attrs_define
class BatchMetadata:
    """Immutable batch metadata

    Attributes:
        batch_id (UUID):
        created_at (datetime.datetime):
        flow_id (str): A type-safe wrapper for blob identifiers.

            Blob IDs are SHA-256 hashes of the content, providing deterministic
            identification and automatic deduplication.
        status (BatchStatus): Batch execution status
        total_inputs (int):
        flow_name (None | str | Unset):
    """

    batch_id: UUID
    created_at: datetime.datetime
    flow_id: str
    status: BatchStatus
    total_inputs: int
    flow_name: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        batch_id = str(self.batch_id)

        created_at = self.created_at.isoformat()

        flow_id = self.flow_id

        status = self.status.value

        total_inputs = self.total_inputs

        flow_name: None | str | Unset
        if isinstance(self.flow_name, Unset):
            flow_name = UNSET
        else:
            flow_name = self.flow_name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "batchId": batch_id,
                "createdAt": created_at,
                "flowId": flow_id,
                "status": status,
                "totalInputs": total_inputs,
            }
        )
        if flow_name is not UNSET:
            field_dict["flowName"] = flow_name

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        batch_id = UUID(d.pop("batchId"))

        created_at = isoparse(d.pop("createdAt"))

        flow_id = d.pop("flowId")

        status = BatchStatus(d.pop("status"))

        total_inputs = d.pop("totalInputs")

        def _parse_flow_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        flow_name = _parse_flow_name(d.pop("flowName", UNSET))

        batch_metadata = cls(
            batch_id=batch_id,
            created_at=created_at,
            flow_id=flow_id,
            status=status,
            total_inputs=total_inputs,
            flow_name=flow_name,
        )

        batch_metadata.additional_properties = d
        return batch_metadata

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
