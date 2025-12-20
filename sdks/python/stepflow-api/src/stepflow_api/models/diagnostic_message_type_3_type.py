from enum import Enum


class DiagnosticMessageType3Type(str, Enum):
    SELFREFERENCE = "selfReference"

    def __str__(self) -> str:
        return str(self.value)
