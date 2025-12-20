from enum import Enum


class DiagnosticMessageType17Type(str, Enum):
    INVALIDROUTEREFERENCE = "invalidRouteReference"

    def __str__(self) -> str:
        return str(self.value)
