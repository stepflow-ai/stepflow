from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.schema_ref import SchemaRef


T = TypeVar("T", bound="ComponentInfo")


@_attrs_define
class ComponentInfo:
    """
    Attributes:
        component (str): Identifies a specific plugin and atomic functionality to execute.

            A component is identified by a path that specifies:
            - The plugin name
            - The component name within that plugin
            - Optional sub-path for specific functionality
        description (None | str | Unset): Optional description of the component.
        input_schema (None | SchemaRef | Unset):
        output_schema (None | SchemaRef | Unset):
    """

    component: str
    description: None | str | Unset = UNSET
    input_schema: None | SchemaRef | Unset = UNSET
    output_schema: None | SchemaRef | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.schema_ref import SchemaRef

        component = self.component

        description: None | str | Unset
        if isinstance(self.description, Unset):
            description = UNSET
        else:
            description = self.description

        input_schema: dict[str, Any] | None | Unset
        if isinstance(self.input_schema, Unset):
            input_schema = UNSET
        elif isinstance(self.input_schema, SchemaRef):
            input_schema = self.input_schema.to_dict()
        else:
            input_schema = self.input_schema

        output_schema: dict[str, Any] | None | Unset
        if isinstance(self.output_schema, Unset):
            output_schema = UNSET
        elif isinstance(self.output_schema, SchemaRef):
            output_schema = self.output_schema.to_dict()
        else:
            output_schema = self.output_schema

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "component": component,
            }
        )
        if description is not UNSET:
            field_dict["description"] = description
        if input_schema is not UNSET:
            field_dict["input_schema"] = input_schema
        if output_schema is not UNSET:
            field_dict["output_schema"] = output_schema

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.schema_ref import SchemaRef

        d = dict(src_dict)
        component = d.pop("component")

        def _parse_description(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        description = _parse_description(d.pop("description", UNSET))

        def _parse_input_schema(data: object) -> None | SchemaRef | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                input_schema_type_1 = SchemaRef.from_dict(data)

                return input_schema_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | SchemaRef | Unset, data)

        input_schema = _parse_input_schema(d.pop("input_schema", UNSET))

        def _parse_output_schema(data: object) -> None | SchemaRef | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                output_schema_type_1 = SchemaRef.from_dict(data)

                return output_schema_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | SchemaRef | Unset, data)

        output_schema = _parse_output_schema(d.pop("output_schema", UNSET))

        component_info = cls(
            component=component,
            description=description,
            input_schema=input_schema,
            output_schema=output_schema,
        )

        component_info.additional_properties = d
        return component_info

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
