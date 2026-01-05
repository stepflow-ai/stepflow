"""Custom CreateRunResponse that accepts flexible result format.

The server returns FlowResult in format: {"outcome": "...", "result"|"error": ...}
But the generated types expect: {"Success": ...} | {"Skipped": ...} | {"Failed": ...}
"""

from typing import Annotated, Any
from uuid import UUID

from pydantic import BaseModel, Field

from .generated import ExecutionStatus


class CreateRunResponse(BaseModel):
    """Custom CreateRunResponse that accepts flexible result format."""

    debug: Annotated[bool, Field(description="Whether this run is in debug mode")]
    result: Any = None  # Accept any format, handle in runtime
    runId: Annotated[UUID, Field(description="The run ID")]
    status: Annotated[ExecutionStatus, Field(description="The run status")]


__all__ = ["CreateRunResponse"]
