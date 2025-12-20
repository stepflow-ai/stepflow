from enum import Enum


class ErrorActionType3Action(str, Enum):
    RETRY = "retry"

    def __str__(self) -> str:
        return str(self.value)
