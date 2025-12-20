from enum import Enum


class DiagnosticMessageType14Type(str, Enum):
    UNVALIDATEDFIELDACCESS = "unvalidatedFieldAccess"

    def __str__(self) -> str:
        return str(self.value)
