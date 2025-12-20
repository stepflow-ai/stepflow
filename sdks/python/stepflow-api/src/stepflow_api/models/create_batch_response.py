from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.batch_status import BatchStatus

T = TypeVar("T", bound="CreateBatchResponse")


@_attrs_define
class CreateBatchResponse:
    """Response for create batch operations

    Attributes:
        batch_id (UUID): The batch ID
        status (BatchStatus): Batch execution status
        total_inputs (int): Total number of inputs in the batch
    """

    batch_id: UUID
    status: BatchStatus
    total_inputs: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        batch_id = str(self.batch_id)

        status = self.status.value

        total_inputs = self.total_inputs

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "batchId": batch_id,
                "status": status,
                "totalInputs": total_inputs,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        batch_id = UUID(d.pop("batchId"))

        status = BatchStatus(d.pop("status"))

        total_inputs = d.pop("totalInputs")

        create_batch_response = cls(
            batch_id=batch_id,
            status=status,
            total_inputs=total_inputs,
        )

        create_batch_response.additional_properties = d
        return create_batch_response

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
