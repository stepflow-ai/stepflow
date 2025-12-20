from enum import Enum


class DiagnosticMessageType0Type(str, Enum):
    DUPLICATESTEPID = "duplicateStepId"

    def __str__(self) -> str:
        return str(self.value)
