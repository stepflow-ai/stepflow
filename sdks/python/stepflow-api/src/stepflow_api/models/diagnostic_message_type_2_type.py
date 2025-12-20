from enum import Enum


class DiagnosticMessageType2Type(str, Enum):
    FORWARDREFERENCE = "forwardReference"

    def __str__(self) -> str:
        return str(self.value)
