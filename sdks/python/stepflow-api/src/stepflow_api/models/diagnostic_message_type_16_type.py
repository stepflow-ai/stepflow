from enum import Enum


class DiagnosticMessageType16Type(str, Enum):
    NOROUTINGRULESCONFIGURED = "noRoutingRulesConfigured"

    def __str__(self) -> str:
        return str(self.value)
