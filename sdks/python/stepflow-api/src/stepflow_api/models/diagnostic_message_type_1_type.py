from enum import Enum


class DiagnosticMessageType1Type(str, Enum):
    EMPTYSTEPID = "emptyStepId"

    def __str__(self) -> str:
        return str(self.value)
