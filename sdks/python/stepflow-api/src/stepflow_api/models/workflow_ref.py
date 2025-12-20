from enum import Enum


class WorkflowRef(str, Enum):
    INPUT = "input"

    def __str__(self) -> str:
        return str(self.value)
