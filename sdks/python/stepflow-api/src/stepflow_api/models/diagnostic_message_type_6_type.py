from enum import Enum


class DiagnosticMessageType6Type(str, Enum):
    INVALIDFIELDACCESS = "invalidFieldAccess"

    def __str__(self) -> str:
        return str(self.value)
