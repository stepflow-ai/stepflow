from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.execution_status import ExecutionStatus
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.flow_result_type_0 import FlowResultType0
    from ..models.flow_result_type_1 import FlowResultType1
    from ..models.flow_result_type_2 import FlowResultType2


T = TypeVar("T", bound="CreateRunResponse")


@_attrs_define
class CreateRunResponse:
    """Response for create run operations

    Attributes:
        debug (bool): Whether this run is in debug mode
        run_id (UUID): The run ID
        status (ExecutionStatus): Status of a workflow execution
        result (FlowResultType0 | FlowResultType1 | FlowResultType2 | None | Unset):
    """

    debug: bool
    run_id: UUID
    status: ExecutionStatus
    result: FlowResultType0 | FlowResultType1 | FlowResultType2 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.flow_result_type_0 import FlowResultType0
        from ..models.flow_result_type_1 import FlowResultType1
        from ..models.flow_result_type_2 import FlowResultType2

        debug = self.debug

        run_id = str(self.run_id)

        status = self.status.value

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
                "debug": debug,
                "runId": run_id,
                "status": status,
            }
        )
        if result is not UNSET:
            field_dict["result"] = result

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.flow_result_type_0 import FlowResultType0
        from ..models.flow_result_type_1 import FlowResultType1
        from ..models.flow_result_type_2 import FlowResultType2

        d = dict(src_dict)
        debug = d.pop("debug")

        run_id = UUID(d.pop("runId"))

        status = ExecutionStatus(d.pop("status"))

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

        create_run_response = cls(
            debug=debug,
            run_id=run_id,
            status=status,
            result=result,
        )

        create_run_response.additional_properties = d
        return create_run_response

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
