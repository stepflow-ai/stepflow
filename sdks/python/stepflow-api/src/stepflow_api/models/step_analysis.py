from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.dependency_type_0 import DependencyType0
    from ..models.dependency_type_1 import DependencyType1
    from ..models.value_dependencies_type_0 import ValueDependenciesType0
    from ..models.value_dependencies_type_1 import ValueDependenciesType1


T = TypeVar("T", bound="StepAnalysis")


@_attrs_define
class StepAnalysis:
    """Analysis for a single step

    Attributes:
        input_depends (ValueDependenciesType0 | ValueDependenciesType1): How a value receives its input data
        skip_if_depend (DependencyType0 | DependencyType1 | None | Unset):
    """

    input_depends: ValueDependenciesType0 | ValueDependenciesType1
    skip_if_depend: DependencyType0 | DependencyType1 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.dependency_type_0 import DependencyType0
        from ..models.dependency_type_1 import DependencyType1
        from ..models.value_dependencies_type_0 import ValueDependenciesType0

        input_depends: dict[str, Any]
        if isinstance(self.input_depends, ValueDependenciesType0):
            input_depends = self.input_depends.to_dict()
        else:
            input_depends = self.input_depends.to_dict()

        skip_if_depend: dict[str, Any] | None | Unset
        if isinstance(self.skip_if_depend, Unset):
            skip_if_depend = UNSET
        elif isinstance(self.skip_if_depend, DependencyType0):
            skip_if_depend = self.skip_if_depend.to_dict()
        elif isinstance(self.skip_if_depend, DependencyType1):
            skip_if_depend = self.skip_if_depend.to_dict()
        else:
            skip_if_depend = self.skip_if_depend

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "inputDepends": input_depends,
            }
        )
        if skip_if_depend is not UNSET:
            field_dict["skipIfDepend"] = skip_if_depend

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.dependency_type_0 import DependencyType0
        from ..models.dependency_type_1 import DependencyType1
        from ..models.value_dependencies_type_0 import ValueDependenciesType0
        from ..models.value_dependencies_type_1 import ValueDependenciesType1

        d = dict(src_dict)

        def _parse_input_depends(data: object) -> ValueDependenciesType0 | ValueDependenciesType1:
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_value_dependencies_type_0 = ValueDependenciesType0.from_dict(data)

                return componentsschemas_value_dependencies_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            if not isinstance(data, dict):
                raise TypeError()
            componentsschemas_value_dependencies_type_1 = ValueDependenciesType1.from_dict(data)

            return componentsschemas_value_dependencies_type_1

        input_depends = _parse_input_depends(d.pop("inputDepends"))

        def _parse_skip_if_depend(data: object) -> DependencyType0 | DependencyType1 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_dependency_type_0 = DependencyType0.from_dict(data)

                return componentsschemas_dependency_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_dependency_type_1 = DependencyType1.from_dict(data)

                return componentsschemas_dependency_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(DependencyType0 | DependencyType1 | None | Unset, data)

        skip_if_depend = _parse_skip_if_depend(d.pop("skipIfDepend", UNSET))

        step_analysis = cls(
            input_depends=input_depends,
            skip_if_depend=skip_if_depend,
        )

        step_analysis.additional_properties = d
        return step_analysis

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
