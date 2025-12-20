from enum import Enum


class SkipActionType0Action(str, Enum):
    SKIP = "skip"

    def __str__(self) -> str:
        return str(self.value)
