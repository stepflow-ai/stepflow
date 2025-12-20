from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.batch_status import BatchStatus

T = TypeVar("T", bound="CancelBatchResponse")


@_attrs_define
class CancelBatchResponse:
    """Response for cancel batch operations

    Attributes:
        batch_id (UUID): The batch ID
        status (BatchStatus): Batch execution status
    """

    batch_id: UUID
    status: BatchStatus
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        batch_id = str(self.batch_id)

        status = self.status.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "batchId": batch_id,
                "status": status,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        batch_id = UUID(d.pop("batchId"))

        status = BatchStatus(d.pop("status"))

        cancel_batch_response = cls(
            batch_id=batch_id,
            status=status,
        )

        cancel_batch_response.additional_properties = d
        return cancel_batch_response

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
