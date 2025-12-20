from enum import Enum


class DiagnosticMessageType4Type(str, Enum):
    UNDEFINEDSTEPREFERENCE = "undefinedStepReference"

    def __str__(self) -> str:
        return str(self.value)
