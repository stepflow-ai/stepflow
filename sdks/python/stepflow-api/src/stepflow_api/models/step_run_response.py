from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.step_status import StepStatus
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.flow_result_type_0 import FlowResultType0
    from ..models.flow_result_type_1 import FlowResultType1
    from ..models.flow_result_type_2 import FlowResultType2


T = TypeVar("T", bound="StepRunResponse")


@_attrs_define
class StepRunResponse:
    """Response for step run details

    Attributes:
        status (StepStatus): Status of an individual step within a workflow
        step_id (str): Step ID
        step_index (int): Step index in the flow
        component (None | str | Unset): Component name/URL that this step executes
        result (FlowResultType0 | FlowResultType1 | FlowResultType2 | None | Unset):
    """

    status: StepStatus
    step_id: str
    step_index: int
    component: None | str | Unset = UNSET
    result: FlowResultType0 | FlowResultType1 | FlowResultType2 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.flow_result_type_0 import FlowResultType0
        from ..models.flow_result_type_1 import FlowResultType1
        from ..models.flow_result_type_2 import FlowResultType2

        status = self.status.value

        step_id = self.step_id

        step_index = self.step_index

        component: None | str | Unset
        if isinstance(self.component, Unset):
            component = UNSET
        else:
            component = self.component

        result: dict[str, Any] | None | Unset
        if isinstance(self.result, Unset):
            result = UNSET
        elif isinstance(self.result, FlowResultType0):
            result = self.result.to_dict()
        elif isinstance(self.result, FlowResultType1):
            result = self.result.to_dict()
        elif isinstance(self.result, FlowResultType2):
            result = self.result.to_dict()
        else:
            result = self.result

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "status": status,
                "stepId": step_id,
                "stepIndex": step_index,
            }
        )
        if component is not UNSET:
            field_dict["component"] = component
        if result is not UNSET:
            field_dict["result"] = result

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.flow_result_type_0 import FlowResultType0
        from ..models.flow_result_type_1 import FlowResultType1
        from ..models.flow_result_type_2 import FlowResultType2

        d = dict(src_dict)
        status = StepStatus(d.pop("status"))

        step_id = d.pop("stepId")

        step_index = d.pop("stepIndex")

        def _parse_component(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        component = _parse_component(d.pop("component", UNSET))

        def _parse_result(data: object) -> FlowResultType0 | FlowResultType1 | FlowResultType2 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
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
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_flow_result_type_2 = FlowResultType2.from_dict(data)

                return componentsschemas_flow_result_type_2
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(FlowResultType0 | FlowResultType1 | FlowResultType2 | None | Unset, data)

        result = _parse_result(d.pop("result", UNSET))

        step_run_response = cls(
            status=status,
            step_id=step_id,
            step_index=step_index,
            component=component,
            result=result,
        )

        step_run_response.additional_properties = d
        return step_run_response

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
