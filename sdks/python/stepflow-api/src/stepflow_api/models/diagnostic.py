from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.diagnostic_level import DiagnosticLevel

if TYPE_CHECKING:
    from ..models.diagnostic_message_type_0 import DiagnosticMessageType0
    from ..models.diagnostic_message_type_1 import DiagnosticMessageType1
    from ..models.diagnostic_message_type_2 import DiagnosticMessageType2
    from ..models.diagnostic_message_type_3 import DiagnosticMessageType3
    from ..models.diagnostic_message_type_4 import DiagnosticMessageType4
    from ..models.diagnostic_message_type_5 import DiagnosticMessageType5
    from ..models.diagnostic_message_type_6 import DiagnosticMessageType6
    from ..models.diagnostic_message_type_7 import DiagnosticMessageType7
    from ..models.diagnostic_message_type_8 import DiagnosticMessageType8
    from ..models.diagnostic_message_type_9 import DiagnosticMessageType9
    from ..models.diagnostic_message_type_10 import DiagnosticMessageType10
    from ..models.diagnostic_message_type_11 import DiagnosticMessageType11
    from ..models.diagnostic_message_type_12 import DiagnosticMessageType12
    from ..models.diagnostic_message_type_13 import DiagnosticMessageType13
    from ..models.diagnostic_message_type_14 import DiagnosticMessageType14
    from ..models.diagnostic_message_type_15 import DiagnosticMessageType15
    from ..models.diagnostic_message_type_16 import DiagnosticMessageType16
    from ..models.diagnostic_message_type_17 import DiagnosticMessageType17
    from ..models.diagnostic_message_type_18 import DiagnosticMessageType18


T = TypeVar("T", bound="Diagnostic")


@_attrs_define
class Diagnostic:
    """A single diagnostic with its context

    Attributes:
        ignore (bool): Whether this diagnostic should be ignored by default
        level (DiagnosticLevel): Diagnostic level indicating severity and impact
        message (DiagnosticMessageType0 | DiagnosticMessageType1 | DiagnosticMessageType10 | DiagnosticMessageType11 |
            DiagnosticMessageType12 | DiagnosticMessageType13 | DiagnosticMessageType14 | DiagnosticMessageType15 |
            DiagnosticMessageType16 | DiagnosticMessageType17 | DiagnosticMessageType18 | DiagnosticMessageType2 |
            DiagnosticMessageType3 | DiagnosticMessageType4 | DiagnosticMessageType5 | DiagnosticMessageType6 |
            DiagnosticMessageType7 | DiagnosticMessageType8 | DiagnosticMessageType9): Specific diagnostic message with
            context
        path (list[str]): JSON path to the field with the issue
        text (str): Human-readable message text
    """

    ignore: bool
    level: DiagnosticLevel
    message: (
        DiagnosticMessageType0
        | DiagnosticMessageType1
        | DiagnosticMessageType10
        | DiagnosticMessageType11
        | DiagnosticMessageType12
        | DiagnosticMessageType13
        | DiagnosticMessageType14
        | DiagnosticMessageType15
        | DiagnosticMessageType16
        | DiagnosticMessageType17
        | DiagnosticMessageType18
        | DiagnosticMessageType2
        | DiagnosticMessageType3
        | DiagnosticMessageType4
        | DiagnosticMessageType5
        | DiagnosticMessageType6
        | DiagnosticMessageType7
        | DiagnosticMessageType8
        | DiagnosticMessageType9
    )
    path: list[str]
    text: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.diagnostic_message_type_0 import DiagnosticMessageType0
        from ..models.diagnostic_message_type_1 import DiagnosticMessageType1
        from ..models.diagnostic_message_type_2 import DiagnosticMessageType2
        from ..models.diagnostic_message_type_3 import DiagnosticMessageType3
        from ..models.diagnostic_message_type_4 import DiagnosticMessageType4
        from ..models.diagnostic_message_type_5 import DiagnosticMessageType5
        from ..models.diagnostic_message_type_6 import DiagnosticMessageType6
        from ..models.diagnostic_message_type_7 import DiagnosticMessageType7
        from ..models.diagnostic_message_type_8 import DiagnosticMessageType8
        from ..models.diagnostic_message_type_9 import DiagnosticMessageType9
        from ..models.diagnostic_message_type_10 import DiagnosticMessageType10
        from ..models.diagnostic_message_type_11 import DiagnosticMessageType11
        from ..models.diagnostic_message_type_12 import DiagnosticMessageType12
        from ..models.diagnostic_message_type_13 import DiagnosticMessageType13
        from ..models.diagnostic_message_type_14 import DiagnosticMessageType14
        from ..models.diagnostic_message_type_15 import DiagnosticMessageType15
        from ..models.diagnostic_message_type_16 import DiagnosticMessageType16
        from ..models.diagnostic_message_type_17 import DiagnosticMessageType17

        ignore = self.ignore

        level = self.level.value

        message: dict[str, Any]
        if isinstance(self.message, DiagnosticMessageType0):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType1):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType2):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType3):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType4):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType5):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType6):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType7):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType8):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType9):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType10):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType11):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType12):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType13):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType14):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType15):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType16):
            message = self.message.to_dict()
        elif isinstance(self.message, DiagnosticMessageType17):
            message = self.message.to_dict()
        else:
            message = self.message.to_dict()

        path = self.path

        text = self.text

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "ignore": ignore,
                "level": level,
                "message": message,
                "path": path,
                "text": text,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.diagnostic_message_type_0 import DiagnosticMessageType0
        from ..models.diagnostic_message_type_1 import DiagnosticMessageType1
        from ..models.diagnostic_message_type_2 import DiagnosticMessageType2
        from ..models.diagnostic_message_type_3 import DiagnosticMessageType3
        from ..models.diagnostic_message_type_4 import DiagnosticMessageType4
        from ..models.diagnostic_message_type_5 import DiagnosticMessageType5
        from ..models.diagnostic_message_type_6 import DiagnosticMessageType6
        from ..models.diagnostic_message_type_7 import DiagnosticMessageType7
        from ..models.diagnostic_message_type_8 import DiagnosticMessageType8
        from ..models.diagnostic_message_type_9 import DiagnosticMessageType9
        from ..models.diagnostic_message_type_10 import DiagnosticMessageType10
        from ..models.diagnostic_message_type_11 import DiagnosticMessageType11
        from ..models.diagnostic_message_type_12 import DiagnosticMessageType12
        from ..models.diagnostic_message_type_13 import DiagnosticMessageType13
        from ..models.diagnostic_message_type_14 import DiagnosticMessageType14
        from ..models.diagnostic_message_type_15 import DiagnosticMessageType15
        from ..models.diagnostic_message_type_16 import DiagnosticMessageType16
        from ..models.diagnostic_message_type_17 import DiagnosticMessageType17
        from ..models.diagnostic_message_type_18 import DiagnosticMessageType18

        d = dict(src_dict)
        ignore = d.pop("ignore")

        level = DiagnosticLevel(d.pop("level"))

        def _parse_message(
            data: object,
        ) -> (
            DiagnosticMessageType0
            | DiagnosticMessageType1
            | DiagnosticMessageType10
            | DiagnosticMessageType11
            | DiagnosticMessageType12
            | DiagnosticMessageType13
            | DiagnosticMessageType14
            | DiagnosticMessageType15
            | DiagnosticMessageType16
            | DiagnosticMessageType17
            | DiagnosticMessageType18
            | DiagnosticMessageType2
            | DiagnosticMessageType3
            | DiagnosticMessageType4
            | DiagnosticMessageType5
            | DiagnosticMessageType6
            | DiagnosticMessageType7
            | DiagnosticMessageType8
            | DiagnosticMessageType9
        ):
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_0 = DiagnosticMessageType0.from_dict(data)

                return componentsschemas_diagnostic_message_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_1 = DiagnosticMessageType1.from_dict(data)

                return componentsschemas_diagnostic_message_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_2 = DiagnosticMessageType2.from_dict(data)

                return componentsschemas_diagnostic_message_type_2
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_3 = DiagnosticMessageType3.from_dict(data)

                return componentsschemas_diagnostic_message_type_3
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_4 = DiagnosticMessageType4.from_dict(data)

                return componentsschemas_diagnostic_message_type_4
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_5 = DiagnosticMessageType5.from_dict(data)

                return componentsschemas_diagnostic_message_type_5
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_6 = DiagnosticMessageType6.from_dict(data)

                return componentsschemas_diagnostic_message_type_6
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_7 = DiagnosticMessageType7.from_dict(data)

                return componentsschemas_diagnostic_message_type_7
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_8 = DiagnosticMessageType8.from_dict(data)

                return componentsschemas_diagnostic_message_type_8
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_9 = DiagnosticMessageType9.from_dict(data)

                return componentsschemas_diagnostic_message_type_9
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_10 = DiagnosticMessageType10.from_dict(data)

                return componentsschemas_diagnostic_message_type_10
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_11 = DiagnosticMessageType11.from_dict(data)

                return componentsschemas_diagnostic_message_type_11
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_12 = DiagnosticMessageType12.from_dict(data)

                return componentsschemas_diagnostic_message_type_12
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_13 = DiagnosticMessageType13.from_dict(data)

                return componentsschemas_diagnostic_message_type_13
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_14 = DiagnosticMessageType14.from_dict(data)

                return componentsschemas_diagnostic_message_type_14
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_15 = DiagnosticMessageType15.from_dict(data)

                return componentsschemas_diagnostic_message_type_15
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_16 = DiagnosticMessageType16.from_dict(data)

                return componentsschemas_diagnostic_message_type_16
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_diagnostic_message_type_17 = DiagnosticMessageType17.from_dict(data)

                return componentsschemas_diagnostic_message_type_17
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            if not isinstance(data, dict):
                raise TypeError()
            componentsschemas_diagnostic_message_type_18 = DiagnosticMessageType18.from_dict(data)

            return componentsschemas_diagnostic_message_type_18

        message = _parse_message(d.pop("message"))

        path = cast(list[str], d.pop("path"))

        text = d.pop("text")

        diagnostic = cls(
            ignore=ignore,
            level=level,
            message=message,
            path=path,
            text=text,
        )

        diagnostic.additional_properties = d
        return diagnostic

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
