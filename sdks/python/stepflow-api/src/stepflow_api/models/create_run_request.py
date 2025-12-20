from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.create_run_request_variables import CreateRunRequestVariables
    from ..models.workflow_overrides import WorkflowOverrides


T = TypeVar("T", bound="CreateRunRequest")


@_attrs_define
class CreateRunRequest:
    """Request to create/execute a flow

    Attributes:
        flow_id (str): A type-safe wrapper for blob identifiers.

            Blob IDs are SHA-256 hashes of the content, providing deterministic
            identification and automatic deduplication.
        input_ (Any): Any JSON value (object, array, string, number, boolean, or null)
        debug (bool | Unset): Whether to run in debug mode (pauses execution for step-by-step control)
        overrides (WorkflowOverrides | Unset): Workflow overrides that can be applied to modify step behavior at
            runtime.

            Overrides are keyed by step ID and contain merge patches or other transformation
            specifications to modify step properties before execution.
        variables (CreateRunRequestVariables | Unset): Optional variables to provide for variable references in the
            workflow
    """

    flow_id: str
    input_: Any
    debug: bool | Unset = UNSET
    overrides: WorkflowOverrides | Unset = UNSET
    variables: CreateRunRequestVariables | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        flow_id = self.flow_id

        input_ = self.input_

        debug = self.debug

        overrides: dict[str, Any] | Unset = UNSET
        if not isinstance(self.overrides, Unset):
            overrides = self.overrides.to_dict()

        variables: dict[str, Any] | Unset = UNSET
        if not isinstance(self.variables, Unset):
            variables = self.variables.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "flowId": flow_id,
                "input": input_,
            }
        )
        if debug is not UNSET:
            field_dict["debug"] = debug
        if overrides is not UNSET:
            field_dict["overrides"] = overrides
        if variables is not UNSET:
            field_dict["variables"] = variables

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.create_run_request_variables import CreateRunRequestVariables
        from ..models.workflow_overrides import WorkflowOverrides

        d = dict(src_dict)
        flow_id = d.pop("flowId")

        input_ = d.pop("input")

        debug = d.pop("debug", UNSET)

        _overrides = d.pop("overrides", UNSET)
        overrides: WorkflowOverrides | Unset
        if isinstance(_overrides, Unset):
            overrides = UNSET
        else:
            overrides = WorkflowOverrides.from_dict(_overrides)

        _variables = d.pop("variables", UNSET)
        variables: CreateRunRequestVariables | Unset
        if isinstance(_variables, Unset):
            variables = UNSET
        else:
            variables = CreateRunRequestVariables.from_dict(_variables)

        create_run_request = cls(
            flow_id=flow_id,
            input_=input_,
            debug=debug,
            overrides=overrides,
            variables=variables,
        )

        create_run_request.additional_properties = d
        return create_run_request

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
