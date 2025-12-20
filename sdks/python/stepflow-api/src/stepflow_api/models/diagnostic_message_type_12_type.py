from enum import Enum


class DiagnosticMessageType12Type(str, Enum):
    MISSINGWORKFLOWNAME = "missingWorkflowName"

    def __str__(self) -> str:
        return str(self.value)
