from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.batch_metadata import BatchMetadata


T = TypeVar("T", bound="ListBatchesResponse")


@_attrs_define
class ListBatchesResponse:
    """Response for listing batches

    Attributes:
        batches (list[BatchMetadata]): List of batch metadata
    """

    batches: list[BatchMetadata]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        batches = []
        for batches_item_data in self.batches:
            batches_item = batches_item_data.to_dict()
            batches.append(batches_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "batches": batches,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.batch_metadata import BatchMetadata

        d = dict(src_dict)
        batches = []
        _batches = d.pop("batches")
        for batches_item_data in _batches:
            batches_item = BatchMetadata.from_dict(batches_item_data)

            batches.append(batches_item)

        list_batches_response = cls(
            batches=batches,
        )

        list_batches_response.additional_properties = d
        return list_batches_response

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
