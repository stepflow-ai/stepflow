from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field
from dateutil.parser import isoparse

from ..models.execution_status import ExecutionStatus
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.flow_result_type_0 import FlowResultType0
    from ..models.flow_result_type_1 import FlowResultType1
    from ..models.flow_result_type_2 import FlowResultType2
    from ..models.workflow_overrides import WorkflowOverrides


T = TypeVar("T", bound="RunDetails")


@_attrs_define
class RunDetails:
    """Detailed flow run information including input and result.

    Attributes:
        created_at (datetime.datetime):
        debug_mode (bool):
        flow_id (str): A type-safe wrapper for blob identifiers.

            Blob IDs are SHA-256 hashes of the content, providing deterministic
            identification and automatic deduplication.
        run_id (UUID):
        status (ExecutionStatus): Status of a workflow execution
        input_ (Any): Any JSON value (object, array, string, number, boolean, or null)
        completed_at (datetime.datetime | None | Unset):
        flow_label (None | str | Unset):
        flow_name (None | str | Unset):
        overrides (None | Unset | WorkflowOverrides):
        result (FlowResultType0 | FlowResultType1 | FlowResultType2 | None | Unset):
    """

    created_at: datetime.datetime
    debug_mode: bool
    flow_id: str
    run_id: UUID
    status: ExecutionStatus
    input_: Any
    completed_at: datetime.datetime | None | Unset = UNSET
    flow_label: None | str | Unset = UNSET
    flow_name: None | str | Unset = UNSET
    overrides: None | Unset | WorkflowOverrides = UNSET
    result: FlowResultType0 | FlowResultType1 | FlowResultType2 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.flow_result_type_0 import FlowResultType0
        from ..models.flow_result_type_1 import FlowResultType1
        from ..models.flow_result_type_2 import FlowResultType2
        from ..models.workflow_overrides import WorkflowOverrides

        created_at = self.created_at.isoformat()

        debug_mode = self.debug_mode

        flow_id = self.flow_id

        run_id = str(self.run_id)

        status = self.status.value

        input_ = self.input_

        completed_at: None | str | Unset
        if isinstance(self.completed_at, Unset):
            completed_at = UNSET
        elif isinstance(self.completed_at, datetime.datetime):
            completed_at = self.completed_at.isoformat()
        else:
            completed_at = self.completed_at

        flow_label: None | str | Unset
        if isinstance(self.flow_label, Unset):
            flow_label = UNSET
        else:
            flow_label = self.flow_label

        flow_name: None | str | Unset
        if isinstance(self.flow_name, Unset):
            flow_name = UNSET
        else:
            flow_name = self.flow_name

        overrides: dict[str, Any] | None | Unset
        if isinstance(self.overrides, Unset):
            overrides = UNSET
        elif isinstance(self.overrides, WorkflowOverrides):
            overrides = self.overrides.to_dict()
        else:
            overrides = self.overrides

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
                "createdAt": created_at,
                "debugMode": debug_mode,
                "flowId": flow_id,
                "runId": run_id,
                "status": status,
                "input": input_,
            }
        )
        if completed_at is not UNSET:
            field_dict["completedAt"] = completed_at
        if flow_label is not UNSET:
            field_dict["flowLabel"] = flow_label
        if flow_name is not UNSET:
            field_dict["flowName"] = flow_name
        if overrides is not UNSET:
            field_dict["overrides"] = overrides
        if result is not UNSET:
            field_dict["result"] = result

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.flow_result_type_0 import FlowResultType0
        from ..models.flow_result_type_1 import FlowResultType1
        from ..models.flow_result_type_2 import FlowResultType2
        from ..models.workflow_overrides import WorkflowOverrides

        d = dict(src_dict)
        created_at = isoparse(d.pop("createdAt"))

        debug_mode = d.pop("debugMode")

        flow_id = d.pop("flowId")

        run_id = UUID(d.pop("runId"))

        status = ExecutionStatus(d.pop("status"))

        input_ = d.pop("input")

        def _parse_completed_at(data: object) -> datetime.datetime | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                completed_at_type_0 = isoparse(data)

                return completed_at_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(datetime.datetime | None | Unset, data)

        completed_at = _parse_completed_at(d.pop("completedAt", UNSET))

        def _parse_flow_label(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        flow_label = _parse_flow_label(d.pop("flowLabel", UNSET))

        def _parse_flow_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        flow_name = _parse_flow_name(d.pop("flowName", UNSET))

        def _parse_overrides(data: object) -> None | Unset | WorkflowOverrides:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                overrides_type_1 = WorkflowOverrides.from_dict(data)

                return overrides_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | Unset | WorkflowOverrides, data)

        overrides = _parse_overrides(d.pop("overrides", UNSET))

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

        run_details = cls(
            created_at=created_at,
            debug_mode=debug_mode,
            flow_id=flow_id,
            run_id=run_id,
            status=status,
            input_=input_,
            completed_at=completed_at,
            flow_label=flow_label,
            flow_name=flow_name,
            overrides=overrides,
            result=result,
        )

        run_details.additional_properties = d
        return run_details

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
