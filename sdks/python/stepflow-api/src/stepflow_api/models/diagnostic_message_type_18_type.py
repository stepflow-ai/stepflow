from enum import Enum


class DiagnosticMessageType18Type(str, Enum):
    UNUSEDPLUGIN = "unusedPlugin"

    def __str__(self) -> str:
        return str(self.value)
