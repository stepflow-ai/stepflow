from enum import Enum


class StepStatus(str, Enum):
    BLOCKED = "blocked"
    COMPLETED = "completed"
    FAILED = "failed"
    RUNNABLE = "runnable"
    RUNNING = "running"
    SKIPPED = "skipped"

    def __str__(self) -> str:
        return str(self.value)
