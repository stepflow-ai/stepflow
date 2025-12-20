from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.dependency_type_0 import DependencyType0
    from ..models.dependency_type_1 import DependencyType1


T = TypeVar("T", bound="ValueDependenciesType1")


@_attrs_define
class ValueDependenciesType1:
    """Value is not an object (single value, array, etc.)

    Attributes:
        other (list[DependencyType0 | DependencyType1]): Value is not an object (single value, array, etc.)
    """

    other: list[DependencyType0 | DependencyType1]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.dependency_type_0 import DependencyType0

        other = []
        for other_item_data in self.other:
            other_item: dict[str, Any]
            if isinstance(other_item_data, DependencyType0):
                other_item = other_item_data.to_dict()
            else:
                other_item = other_item_data.to_dict()

            other.append(other_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "other": other,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.dependency_type_0 import DependencyType0
        from ..models.dependency_type_1 import DependencyType1

        d = dict(src_dict)
        other = []
        _other = d.pop("other")
        for other_item_data in _other:

            def _parse_other_item(data: object) -> DependencyType0 | DependencyType1:
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

            other_item = _parse_other_item(other_item_data)

            other.append(other_item)

        value_dependencies_type_1 = cls(
            other=other,
        )

        value_dependencies_type_1.additional_properties = d
        return value_dependencies_type_1

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
