from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.batch_status import BatchStatus
from ..types import UNSET, Unset

T = TypeVar("T", bound="ListBatchesQuery")


@_attrs_define
class ListBatchesQuery:
    """Query parameters for listing batches

    Attributes:
        flow_name (None | str | Unset): Filter by flow name
        limit (int | None | Unset): Maximum number of results to return
        offset (int | None | Unset): Number of results to skip
        status (BatchStatus | None | Unset):
    """

    flow_name: None | str | Unset = UNSET
    limit: int | None | Unset = UNSET
    offset: int | None | Unset = UNSET
    status: BatchStatus | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        flow_name: None | str | Unset
        if isinstance(self.flow_name, Unset):
            flow_name = UNSET
        else:
            flow_name = self.flow_name

        limit: int | None | Unset
        if isinstance(self.limit, Unset):
            limit = UNSET
        else:
            limit = self.limit

        offset: int | None | Unset
        if isinstance(self.offset, Unset):
            offset = UNSET
        else:
            offset = self.offset

        status: None | str | Unset
        if isinstance(self.status, Unset):
            status = UNSET
        elif isinstance(self.status, BatchStatus):
            status = self.status.value
        else:
            status = self.status

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if flow_name is not UNSET:
            field_dict["flowName"] = flow_name
        if limit is not UNSET:
            field_dict["limit"] = limit
        if offset is not UNSET:
            field_dict["offset"] = offset
        if status is not UNSET:
            field_dict["status"] = status

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)

        def _parse_flow_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        flow_name = _parse_flow_name(d.pop("flowName", UNSET))

        def _parse_limit(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        limit = _parse_limit(d.pop("limit", UNSET))

        def _parse_offset(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        offset = _parse_offset(d.pop("offset", UNSET))

        def _parse_status(data: object) -> BatchStatus | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                status_type_1 = BatchStatus(data)

                return status_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(BatchStatus | None | Unset, data)

        status = _parse_status(d.pop("status", UNSET))

        list_batches_query = cls(
            flow_name=flow_name,
            limit=limit,
            offset=offset,
            status=status,
        )

        list_batches_query.additional_properties = d
        return list_batches_query

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
