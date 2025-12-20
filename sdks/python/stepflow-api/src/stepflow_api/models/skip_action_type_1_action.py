from enum import Enum


class SkipActionType1Action(str, Enum):
    USEDEFAULT = "useDefault"

    def __str__(self) -> str:
        return str(self.value)
