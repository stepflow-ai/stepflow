"""Custom RunDetails that accepts flexible result format.

The server returns FlowResult in format: {"outcome": "...", "result"|"error": ...}
But the generated types expect: {"Success": ...} | {"Skipped": ...} | {"Failed": ...}
"""

from typing import Annotated, Any
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field

from .generated import ExecutionStatus


class RunDetails(BaseModel):
    """Custom RunDetails that accepts flexible result format."""

    model_config = ConfigDict(extra="ignore")  # Ignore extra fields from server

    completedAt: str | None = None
    createdAt: str | None = None
    debugMode: bool = False  # Server uses debugMode, not debug
    flowId: str
    result: Any = None  # Accept any format, handle in runtime
    runId: UUID
    status: ExecutionStatus
    startedAt: str | None = None
    updatedAt: str | None = None

    @property
    def debug(self) -> bool:
        """Alias for debugMode for API compatibility."""
        return self.debugMode


__all__ = ["RunDetails"]
