from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.flow_error import FlowError


T = TypeVar("T", bound="FlowResultType2")


@_attrs_define
class FlowResultType2:
    """# Failed
    The step failed with the given error.

        Attributes:
            failed (FlowError): An error reported from within a flow or step.
    """

    failed: FlowError
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        failed = self.failed.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "Failed": failed,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.flow_error import FlowError

        d = dict(src_dict)
        failed = FlowError.from_dict(d.pop("Failed"))

        flow_result_type_2 = cls(
            failed=failed,
        )

        flow_result_type_2.additional_properties = d
        return flow_result_type_2

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
