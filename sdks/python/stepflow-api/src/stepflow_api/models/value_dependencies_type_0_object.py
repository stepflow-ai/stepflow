from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.dependency_type_0 import DependencyType0
    from ..models.dependency_type_1 import DependencyType1


T = TypeVar("T", bound="ValueDependenciesType0Object")


@_attrs_define
class ValueDependenciesType0Object:
    """Value is an object with named fields"""

    additional_properties: dict[str, list[DependencyType0 | DependencyType1]] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.dependency_type_0 import DependencyType0

        field_dict: dict[str, Any] = {}
        for prop_name, prop in self.additional_properties.items():
            field_dict[prop_name] = []
            for additional_property_item_data in prop:
                additional_property_item: dict[str, Any]
                if isinstance(additional_property_item_data, DependencyType0):
                    additional_property_item = additional_property_item_data.to_dict()
                else:
                    additional_property_item = additional_property_item_data.to_dict()

                field_dict[prop_name].append(additional_property_item)

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.dependency_type_0 import DependencyType0
        from ..models.dependency_type_1 import DependencyType1

        d = dict(src_dict)
        value_dependencies_type_0_object = cls()

        additional_properties = {}
        for prop_name, prop_dict in d.items():
            additional_property = []
            _additional_property = prop_dict
            for additional_property_item_data in _additional_property:

                def _parse_additional_property_item(data: object) -> DependencyType0 | DependencyType1:
                    try:
                        if not isinstance(data, dict):
                            raise TypeError()
                        componentsschemas_dependency_type_0 = DependencyType0.from_dict(data)

                        return componentsschemas_dependency_type_0
                    except (TypeError, ValueError, AttributeError, KeyError):
                        pass
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_dependency_type_1 = DependencyType1.from_dict(data)

                    return componentsschemas_dependency_type_1

                additional_property_item = _parse_additional_property_item(additional_property_item_data)

                additional_property.append(additional_property_item)

            additional_properties[prop_name] = additional_property

        value_dependencies_type_0_object.additional_properties = additional_properties
        return value_dependencies_type_0_object

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> list[DependencyType0 | DependencyType1]:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: list[DependencyType0 | DependencyType1]) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
