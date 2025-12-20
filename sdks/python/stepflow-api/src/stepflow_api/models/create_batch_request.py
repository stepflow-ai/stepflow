from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.workflow_overrides import WorkflowOverrides


T = TypeVar("T", bound="CreateBatchRequest")


@_attrs_define
class CreateBatchRequest:
    """Request to create a batch execution

    Attributes:
        flow_id (str): A type-safe wrapper for blob identifiers.

            Blob IDs are SHA-256 hashes of the content, providing deterministic
            identification and automatic deduplication.
        inputs (list[Any]): Array of input data for each run in the batch
        max_concurrency (int | None | Unset): Maximum number of concurrent executions (defaults to number of inputs if
            not specified)
        overrides (WorkflowOverrides | Unset): Workflow overrides that can be applied to modify step behavior at
            runtime.

            Overrides are keyed by step ID and contain merge patches or other transformation
            specifications to modify step properties before execution.
    """

    flow_id: str
    inputs: list[Any]
    max_concurrency: int | None | Unset = UNSET
    overrides: WorkflowOverrides | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        flow_id = self.flow_id

        inputs = self.inputs

        max_concurrency: int | None | Unset
        if isinstance(self.max_concurrency, Unset):
            max_concurrency = UNSET
        else:
            max_concurrency = self.max_concurrency

        overrides: dict[str, Any] | Unset = UNSET
        if not isinstance(self.overrides, Unset):
            overrides = self.overrides.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "flowId": flow_id,
                "inputs": inputs,
            }
        )
        if max_concurrency is not UNSET:
            field_dict["maxConcurrency"] = max_concurrency
        if overrides is not UNSET:
            field_dict["overrides"] = overrides

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.workflow_overrides import WorkflowOverrides

        d = dict(src_dict)
        flow_id = d.pop("flowId")

        inputs = cast(list[Any], d.pop("inputs"))

        def _parse_max_concurrency(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        max_concurrency = _parse_max_concurrency(d.pop("maxConcurrency", UNSET))

        _overrides = d.pop("overrides", UNSET)
        overrides: WorkflowOverrides | Unset
        if isinstance(_overrides, Unset):
            overrides = UNSET
        else:
            overrides = WorkflowOverrides.from_dict(_overrides)

        create_batch_request = cls(
            flow_id=flow_id,
            inputs=inputs,
            max_concurrency=max_concurrency,
            overrides=overrides,
        )

        create_batch_request.additional_properties = d
        return create_batch_request

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
