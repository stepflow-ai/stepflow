from enum import Enum


class DiagnosticMessageType9Type(str, Enum):
    SCHEMAVIOLATION = "schemaViolation"

    def __str__(self) -> str:
        return str(self.value)
