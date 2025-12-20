"""FlowResultType1 - Skipped result type.

This is the Skipped variant of FlowResult.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.flow_result_type_1_skipped import FlowResultType1Skipped


T = TypeVar("T", bound="FlowResultType1")


@_attrs_define
class FlowResultType1:
    """# Skipped
    The step was skipped.

    Attributes:
        skipped (FlowResultType1Skipped): The skipped result details
    """

    skipped: FlowResultType1Skipped
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.flow_result_type_1_skipped import FlowResultType1Skipped

        skipped = self.skipped.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "Skipped": skipped,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.flow_result_type_1_skipped import FlowResultType1Skipped

        d = dict(src_dict)
        skipped = FlowResultType1Skipped.from_dict(d.pop("Skipped"))

        flow_result_type_1 = cls(
            skipped=skipped,
        )

        flow_result_type_1.additional_properties = d
        return flow_result_type_1

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
