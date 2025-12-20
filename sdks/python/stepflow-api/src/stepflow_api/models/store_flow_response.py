from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.diagnostics import Diagnostics
    from ..models.flow_analysis import FlowAnalysis


T = TypeVar("T", bound="StoreFlowResponse")


@_attrs_define
class StoreFlowResponse:
    """Response when a flow is stored

    Attributes:
        diagnostics (Diagnostics): Collection of diagnostics with utility methods
        analysis (FlowAnalysis | None | Unset):
        flow_id (None | str | Unset):
    """

    diagnostics: Diagnostics
    analysis: FlowAnalysis | None | Unset = UNSET
    flow_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.flow_analysis import FlowAnalysis

        diagnostics = self.diagnostics.to_dict()

        analysis: dict[str, Any] | None | Unset
        if isinstance(self.analysis, Unset):
            analysis = UNSET
        elif isinstance(self.analysis, FlowAnalysis):
            analysis = self.analysis.to_dict()
        else:
            analysis = self.analysis

        flow_id: None | str | Unset
        if isinstance(self.flow_id, Unset):
            flow_id = UNSET
        else:
            flow_id = self.flow_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "diagnostics": diagnostics,
            }
        )
        if analysis is not UNSET:
            field_dict["analysis"] = analysis
        if flow_id is not UNSET:
            field_dict["flowId"] = flow_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.diagnostics import Diagnostics
        from ..models.flow_analysis import FlowAnalysis

        d = dict(src_dict)
        diagnostics = Diagnostics.from_dict(d.pop("diagnostics"))

        def _parse_analysis(data: object) -> FlowAnalysis | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                analysis_type_1 = FlowAnalysis.from_dict(data)

                return analysis_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(FlowAnalysis | None | Unset, data)

        analysis = _parse_analysis(d.pop("analysis", UNSET))

        def _parse_flow_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        flow_id = _parse_flow_id(d.pop("flowId", UNSET))

        store_flow_response = cls(
            diagnostics=diagnostics,
            analysis=analysis,
            flow_id=flow_id,
        )

        store_flow_response.additional_properties = d
        return store_flow_response

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
