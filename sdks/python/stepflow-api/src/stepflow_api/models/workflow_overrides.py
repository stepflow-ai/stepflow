from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.workflow_overrides_steps import WorkflowOverridesSteps


T = TypeVar("T", bound="WorkflowOverrides")


@_attrs_define
class WorkflowOverrides:
    """Workflow overrides that can be applied to modify step behavior at runtime.

    Overrides are keyed by step ID and contain merge patches or other transformation
    specifications to modify step properties before execution.

        Attributes:
            steps (WorkflowOverridesSteps): Map of step ID to override specification
    """

    steps: WorkflowOverridesSteps
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        # Server uses #[serde(transparent)] so we return just the step map
        # without the "steps" wrapper
        steps = self.steps.to_dict()
        return steps

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.workflow_overrides_steps import WorkflowOverridesSteps

        d = dict(src_dict)
        steps = WorkflowOverridesSteps.from_dict(d.pop("steps"))

        workflow_overrides = cls(
            steps=steps,
        )

        workflow_overrides.additional_properties = d
        return workflow_overrides

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
