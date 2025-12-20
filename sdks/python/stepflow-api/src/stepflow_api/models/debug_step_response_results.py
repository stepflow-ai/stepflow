from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.flow_result_type_0 import FlowResultType0
    from ..models.flow_result_type_1 import FlowResultType1
    from ..models.flow_result_type_2 import FlowResultType2


T = TypeVar("T", bound="DebugStepResponseResults")


@_attrs_define
class DebugStepResponseResults:
    """Results of executed steps"""

    additional_properties: dict[str, FlowResultType0 | FlowResultType1 | FlowResultType2] = _attrs_field(
        init=False, factory=dict
    )

    def to_dict(self) -> dict[str, Any]:
        from ..models.flow_result_type_0 import FlowResultType0
        from ..models.flow_result_type_1 import FlowResultType1

        field_dict: dict[str, Any] = {}
        for prop_name, prop in self.additional_properties.items():
            if isinstance(prop, FlowResultType0):
                field_dict[prop_name] = prop.to_dict()
            elif isinstance(prop, FlowResultType1):
                field_dict[prop_name] = prop.to_dict()
            else:
                field_dict[prop_name] = prop.to_dict()

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.flow_result_type_0 import FlowResultType0
        from ..models.flow_result_type_1 import FlowResultType1
        from ..models.flow_result_type_2 import FlowResultType2

        d = dict(src_dict)
        debug_step_response_results = cls()

        additional_properties = {}
        for prop_name, prop_dict in d.items():

            def _parse_additional_property(data: object) -> FlowResultType0 | FlowResultType1 | FlowResultType2:
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_flow_result_type_0 = FlowResultType0.from_dict(data)

                    return componentsschemas_flow_result_type_0
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_flow_result_type_1 = FlowResultType1.from_dict(data)

                    return componentsschemas_flow_result_type_1
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_flow_result_type_2 = FlowResultType2.from_dict(data)

                return componentsschemas_flow_result_type_2

            additional_property = _parse_additional_property(prop_dict)

            additional_properties[prop_name] = additional_property

        debug_step_response_results.additional_properties = additional_properties
        return debug_step_response_results

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> FlowResultType0 | FlowResultType1 | FlowResultType2:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: FlowResultType0 | FlowResultType1 | FlowResultType2) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
