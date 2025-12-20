from enum import Enum


class ErrorActionType0Action(str, Enum):
    FAIL = "fail"

    def __str__(self) -> str:
        return str(self.value)
