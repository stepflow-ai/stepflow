from enum import Enum


class DiagnosticMessageType5Type(str, Enum):
    INVALIDREFERENCEEXPRESSION = "invalidReferenceExpression"

    def __str__(self) -> str:
        return str(self.value)
