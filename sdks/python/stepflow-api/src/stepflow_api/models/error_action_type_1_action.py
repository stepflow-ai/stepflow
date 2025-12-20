from enum import Enum


class ErrorActionType1Action(str, Enum):
    SKIP = "skip"

    def __str__(self) -> str:
        return str(self.value)
