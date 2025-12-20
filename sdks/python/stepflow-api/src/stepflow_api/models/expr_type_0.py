from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.base_ref_type_0 import BaseRefType0
    from ..models.base_ref_type_1 import BaseRefType1
    from ..models.base_ref_type_2 import BaseRefType2
    from ..models.skip_action_type_0 import SkipActionType0
    from ..models.skip_action_type_1 import SkipActionType1


T = TypeVar("T", bound="ExprType0")


@_attrs_define
class ExprType0:
    """# Reference
    Reference a value from a step, workflow, or other source.

        Attributes:
            from_ (BaseRefType0 | BaseRefType1 | BaseRefType2): An expression that can be either a literal value or a
                template expression.
            on_skip (None | SkipActionType0 | SkipActionType1 | Unset):
            path (Any | Unset): JSON path expression to apply to the referenced value. May use `$` to reference the whole
                value. May also be a bare field name (without the leading $) if the referenced value is an object.
    """

    from_: BaseRefType0 | BaseRefType1 | BaseRefType2
    on_skip: None | SkipActionType0 | SkipActionType1 | Unset = UNSET
    path: Any | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.base_ref_type_0 import BaseRefType0
        from ..models.base_ref_type_1 import BaseRefType1
        from ..models.skip_action_type_0 import SkipActionType0
        from ..models.skip_action_type_1 import SkipActionType1

        from_: dict[str, Any]
        if isinstance(self.from_, BaseRefType0):
            from_ = self.from_.to_dict()
        elif isinstance(self.from_, BaseRefType1):
            from_ = self.from_.to_dict()
        else:
            from_ = self.from_.to_dict()

        on_skip: dict[str, Any] | None | Unset
        if isinstance(self.on_skip, Unset):
            on_skip = UNSET
        elif isinstance(self.on_skip, SkipActionType0):
            on_skip = self.on_skip.to_dict()
        elif isinstance(self.on_skip, SkipActionType1):
            on_skip = self.on_skip.to_dict()
        else:
            on_skip = self.on_skip

        path = self.path

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "$from": from_,
            }
        )
        if on_skip is not UNSET:
            field_dict["onSkip"] = on_skip
        if path is not UNSET:
            field_dict["path"] = path

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.base_ref_type_0 import BaseRefType0
        from ..models.base_ref_type_1 import BaseRefType1
        from ..models.base_ref_type_2 import BaseRefType2
        from ..models.skip_action_type_0 import SkipActionType0
        from ..models.skip_action_type_1 import SkipActionType1

        d = dict(src_dict)

        def _parse_from_(data: object) -> BaseRefType0 | BaseRefType1 | BaseRefType2:
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_base_ref_type_0 = BaseRefType0.from_dict(data)

                return componentsschemas_base_ref_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_base_ref_type_1 = BaseRefType1.from_dict(data)

                return componentsschemas_base_ref_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            if not isinstance(data, dict):
                raise TypeError()
            componentsschemas_base_ref_type_2 = BaseRefType2.from_dict(data)

            return componentsschemas_base_ref_type_2

        from_ = _parse_from_(d.pop("$from"))

        def _parse_on_skip(data: object) -> None | SkipActionType0 | SkipActionType1 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_skip_action_type_0 = SkipActionType0.from_dict(data)

                return componentsschemas_skip_action_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_skip_action_type_1 = SkipActionType1.from_dict(data)

                return componentsschemas_skip_action_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | SkipActionType0 | SkipActionType1 | Unset, data)

        on_skip = _parse_on_skip(d.pop("onSkip", UNSET))

        path = d.pop("path", UNSET)

        expr_type_0 = cls(
            from_=from_,
            on_skip=on_skip,
            path=path,
        )

        expr_type_0.additional_properties = d
        return expr_type_0

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
