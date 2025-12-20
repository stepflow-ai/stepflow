from enum import Enum


class DiagnosticMessageType8Type(str, Enum):
    EMPTYCOMPONENTNAME = "emptyComponentName"

    def __str__(self) -> str:
        return str(self.value)
