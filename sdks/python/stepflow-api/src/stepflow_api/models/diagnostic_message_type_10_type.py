from enum import Enum


class DiagnosticMessageType10Type(str, Enum):
    MOCKCOMPONENT = "mockComponent"

    def __str__(self) -> str:
        return str(self.value)
