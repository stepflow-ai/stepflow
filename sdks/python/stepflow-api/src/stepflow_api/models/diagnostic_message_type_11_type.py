from enum import Enum


class DiagnosticMessageType11Type(str, Enum):
    UNREACHABLESTEP = "unreachableStep"

    def __str__(self) -> str:
        return str(self.value)
