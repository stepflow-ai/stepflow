from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.error_action_type_0 import ErrorActionType0
    from ..models.error_action_type_1 import ErrorActionType1
    from ..models.error_action_type_2 import ErrorActionType2
    from ..models.error_action_type_3 import ErrorActionType3
    from ..models.expr_type_0 import ExprType0
    from ..models.expr_type_1 import ExprType1
    from ..models.schema_ref import SchemaRef
    from ..models.step_metadata import StepMetadata


T = TypeVar("T", bound="Step")


@_attrs_define
class Step:
    """A step in a workflow that executes a component with specific arguments.

    Attributes:
        component (str): Identifies a specific plugin and atomic functionality to execute.

            A component is identified by a path that specifies:
            - The plugin name
            - The component name within that plugin
            - Optional sub-path for specific functionality
        id (str): Identifier for the step
        input_ (Any | Unset): A value that can be either a literal JSON value or an expression that references other
            values using the $from syntax
        input_schema (None | SchemaRef | Unset):
        metadata (StepMetadata | Unset): Extensible metadata for the step that can be used by tools and frameworks.
        must_execute (bool | None | Unset): If true, this step must execute even if its output is not used by the
            workflow output.
            Useful for steps with side effects (e.g., writing to databases, sending notifications).

            Note: If the step has `skip_if` that evaluates to true, the step will still be skipped
            and its dependencies will not be forced to execute.
        on_error (ErrorActionType0 | ErrorActionType1 | ErrorActionType2 | ErrorActionType3 | None | Unset):
        output_schema (None | SchemaRef | Unset):
        skip_if (Any | ExprType0 | ExprType1 | None | Unset):
    """

    component: str
    id: str
    input_: Any | Unset = UNSET
    input_schema: None | SchemaRef | Unset = UNSET
    metadata: StepMetadata | Unset = UNSET
    must_execute: bool | None | Unset = UNSET
    on_error: ErrorActionType0 | ErrorActionType1 | ErrorActionType2 | ErrorActionType3 | None | Unset = UNSET
    output_schema: None | SchemaRef | Unset = UNSET
    skip_if: Any | ExprType0 | ExprType1 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.error_action_type_0 import ErrorActionType0
        from ..models.error_action_type_1 import ErrorActionType1
        from ..models.error_action_type_2 import ErrorActionType2
        from ..models.error_action_type_3 import ErrorActionType3
        from ..models.expr_type_0 import ExprType0
        from ..models.expr_type_1 import ExprType1
        from ..models.schema_ref import SchemaRef

        component = self.component

        id = self.id

        input_ = self.input_

        input_schema: dict[str, Any] | None | Unset
        if isinstance(self.input_schema, Unset):
            input_schema = UNSET
        elif isinstance(self.input_schema, SchemaRef):
            input_schema = self.input_schema.to_dict()
        else:
            input_schema = self.input_schema

        metadata: dict[str, Any] | Unset = UNSET
        if not isinstance(self.metadata, Unset):
            metadata = self.metadata.to_dict()

        must_execute: bool | None | Unset
        if isinstance(self.must_execute, Unset):
            must_execute = UNSET
        else:
            must_execute = self.must_execute

        on_error: dict[str, Any] | None | Unset
        if isinstance(self.on_error, Unset):
            on_error = UNSET
        elif isinstance(self.on_error, ErrorActionType0):
            on_error = self.on_error.to_dict()
        elif isinstance(self.on_error, ErrorActionType1):
            on_error = self.on_error.to_dict()
        elif isinstance(self.on_error, ErrorActionType2):
            on_error = self.on_error.to_dict()
        elif isinstance(self.on_error, ErrorActionType3):
            on_error = self.on_error.to_dict()
        else:
            on_error = self.on_error

        output_schema: dict[str, Any] | None | Unset
        if isinstance(self.output_schema, Unset):
            output_schema = UNSET
        elif isinstance(self.output_schema, SchemaRef):
            output_schema = self.output_schema.to_dict()
        else:
            output_schema = self.output_schema

        skip_if: Any | dict[str, Any] | None | Unset
        if isinstance(self.skip_if, Unset):
            skip_if = UNSET
        elif isinstance(self.skip_if, ExprType0):
            skip_if = self.skip_if.to_dict()
        elif isinstance(self.skip_if, ExprType1):
            skip_if = self.skip_if.to_dict()
        else:
            skip_if = self.skip_if

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "component": component,
                "id": id,
            }
        )
        if input_ is not UNSET:
            field_dict["input"] = input_
        if input_schema is not UNSET:
            field_dict["inputSchema"] = input_schema
        if metadata is not UNSET:
            field_dict["metadata"] = metadata
        if must_execute is not UNSET:
            field_dict["mustExecute"] = must_execute
        if on_error is not UNSET:
            field_dict["onError"] = on_error
        if output_schema is not UNSET:
            field_dict["outputSchema"] = output_schema
        if skip_if is not UNSET:
            field_dict["skipIf"] = skip_if

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.error_action_type_0 import ErrorActionType0
        from ..models.error_action_type_1 import ErrorActionType1
        from ..models.error_action_type_2 import ErrorActionType2
        from ..models.error_action_type_3 import ErrorActionType3
        from ..models.expr_type_0 import ExprType0
        from ..models.expr_type_1 import ExprType1
        from ..models.schema_ref import SchemaRef
        from ..models.step_metadata import StepMetadata

        d = dict(src_dict)
        component = d.pop("component")

        id = d.pop("id")

        input_ = d.pop("input", UNSET)

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

        input_schema = _parse_input_schema(d.pop("inputSchema", UNSET))

        _metadata = d.pop("metadata", UNSET)
        metadata: StepMetadata | Unset
        if isinstance(_metadata, Unset):
            metadata = UNSET
        else:
            metadata = StepMetadata.from_dict(_metadata)

        def _parse_must_execute(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        must_execute = _parse_must_execute(d.pop("mustExecute", UNSET))

        def _parse_on_error(
            data: object,
        ) -> ErrorActionType0 | ErrorActionType1 | ErrorActionType2 | ErrorActionType3 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_error_action_type_0 = ErrorActionType0.from_dict(data)

                return componentsschemas_error_action_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_error_action_type_1 = ErrorActionType1.from_dict(data)

                return componentsschemas_error_action_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_error_action_type_2 = ErrorActionType2.from_dict(data)

                return componentsschemas_error_action_type_2
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_error_action_type_3 = ErrorActionType3.from_dict(data)

                return componentsschemas_error_action_type_3
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(ErrorActionType0 | ErrorActionType1 | ErrorActionType2 | ErrorActionType3 | None | Unset, data)

        on_error = _parse_on_error(d.pop("onError", UNSET))

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

        output_schema = _parse_output_schema(d.pop("outputSchema", UNSET))

        def _parse_skip_if(data: object) -> Any | ExprType0 | ExprType1 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_expr_type_0 = ExprType0.from_dict(data)

                return componentsschemas_expr_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_expr_type_1 = ExprType1.from_dict(data)

                return componentsschemas_expr_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(Any | ExprType0 | ExprType1 | None | Unset, data)

        skip_if = _parse_skip_if(d.pop("skipIf", UNSET))

        step = cls(
            component=component,
            id=id,
            input_=input_,
            input_schema=input_schema,
            metadata=metadata,
            must_execute=must_execute,
            on_error=on_error,
            output_schema=output_schema,
            skip_if=skip_if,
        )

        step.additional_properties = d
        return step

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
