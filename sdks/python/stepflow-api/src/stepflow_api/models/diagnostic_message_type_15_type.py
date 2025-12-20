from enum import Enum


class DiagnosticMessageType15Type(str, Enum):
    NOPLUGINSCONFIGURED = "noPluginsConfigured"

    def __str__(self) -> str:
        return str(self.value)
