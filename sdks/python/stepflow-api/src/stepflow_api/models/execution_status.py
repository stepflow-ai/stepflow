from enum import Enum


class ExecutionStatus(str, Enum):
    CANCELLED = "cancelled"
    COMPLETED = "completed"
    FAILED = "failed"
    PAUSED = "paused"
    RUNNING = "running"

    def __str__(self) -> str:
        return str(self.value)
