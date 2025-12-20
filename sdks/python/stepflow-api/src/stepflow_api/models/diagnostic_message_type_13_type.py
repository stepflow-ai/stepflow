from enum import Enum


class DiagnosticMessageType13Type(str, Enum):
    MISSINGWORKFLOWDESCRIPTION = "missingWorkflowDescription"

    def __str__(self) -> str:
        return str(self.value)
