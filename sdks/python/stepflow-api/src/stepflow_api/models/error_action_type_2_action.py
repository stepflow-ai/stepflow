from enum import Enum


class ErrorActionType2Action(str, Enum):
    USEDEFAULT = "useDefault"

    def __str__(self) -> str:
        return str(self.value)
