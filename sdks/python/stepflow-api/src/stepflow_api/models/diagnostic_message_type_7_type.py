from enum import Enum


class DiagnosticMessageType7Type(str, Enum):
    INVALIDCOMPONENT = "invalidComponent"

    def __str__(self) -> str:
        return str(self.value)
