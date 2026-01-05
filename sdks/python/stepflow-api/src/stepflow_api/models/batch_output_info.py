"""Custom BatchOutputInfo that accepts flexible result format.

The server returns FlowResult in format: {"outcome": "...", "result"|"error": ...}
But the generated types expect: {"Success": ...} | {"Skipped": ...} | {"Failed": ...}
"""

from typing import Annotated, Any

from pydantic import BaseModel, Field

from .generated import ExecutionStatus


class BatchOutputInfo(BaseModel):
    """Custom BatchOutputInfo that accepts flexible result format."""

    batchInputIndex: Annotated[
        int, Field(description="Position in the batch input array", ge=0)
    ]
    result: Any = None  # Accept any format, handle in runtime
    status: Annotated[ExecutionStatus, Field(description="The execution status")]


__all__ = ["BatchOutputInfo"]
