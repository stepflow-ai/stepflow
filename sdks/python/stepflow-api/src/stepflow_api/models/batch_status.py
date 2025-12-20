from enum import Enum


class BatchStatus(str, Enum):
    CANCELLED = "cancelled"
    RUNNING = "running"

    def __str__(self) -> str:
        return str(self.value)
