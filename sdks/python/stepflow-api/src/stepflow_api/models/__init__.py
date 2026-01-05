"""Generated models for the Stepflow API.

This module re-exports all generated models and adds compatibility
methods (to_dict, from_dict) for backwards compatibility.

Custom overrides are provided for models where the generated types
don't match the server's actual response format.
"""

from __future__ import annotations

import sys
from typing import Annotated, Any
from uuid import UUID

from pydantic import BaseModel, Field

# Import all from generated
from .generated import *  # noqa: F401, F403

# Import specific types we need for overrides
from .generated import ExecutionStatus, BatchStatistics, BatchStatus

# Collect all exported names for __all__
__all__: list[str] = []

# Get all classes from generated module
from . import generated

for name in dir(generated):
    obj = getattr(generated, name)
    if isinstance(obj, type):
        __all__.append(name)

# Re-export UNSET from types (add to __all__ first to satisfy linter)
__all__.extend(["UNSET", "Unset"])
from ..types import UNSET, Unset  # noqa: E402, F401

# Custom overrides for models that need special handling
# Import these AFTER the generated imports to override them
from .workflow_overrides import WorkflowOverrides  # noqa: E402, F401
from .create_run_request import CreateRunRequest  # noqa: E402, F401
from .create_run_response import CreateRunResponse  # noqa: E402, F401
from .run_details import RunDetails  # noqa: E402, F401
from .batch_output_info import BatchOutputInfo  # noqa: E402, F401


# =============================================================================
# Custom model overrides
# The server returns FlowResult in format: {"outcome": "...", "result"|"error": ...}
# But the generated types expect: {"Success": ...} | {"Skipped": ...} | {"Failed": ...}
# We override the response models to accept Any for the result field.
# =============================================================================


class CreateRunResponse(BaseModel):
    """Custom CreateRunResponse that accepts flexible result format."""

    debug: Annotated[bool, Field(description="Whether this run is in debug mode")]
    result: Any = None  # Accept any format, handle in runtime
    runId: Annotated[UUID, Field(description="The run ID")]
    status: Annotated[ExecutionStatus, Field(description="The run status")]


class RunDetails(BaseModel):
    """Custom RunDetails that accepts flexible result format."""

    createdAt: Annotated[str | None, Field(description="When the run was created")] = None
    debug: Annotated[bool, Field(description="Whether this run is in debug mode")]
    flowId: Annotated[str, Field(description="The flow ID")]
    result: Any = None  # Accept any format, handle in runtime
    runId: Annotated[UUID, Field(description="The run ID")]
    status: Annotated[ExecutionStatus, Field(description="The run status")]
    updatedAt: Annotated[str | None, Field(description="When the run was last updated")] = None


class BatchOutputInfo(BaseModel):
    """Custom BatchOutputInfo that accepts flexible result format."""

    error: str | None = None
    index: Annotated[int, Field(ge=0)]
    result: Any = None  # Accept any format, handle in runtime


class ListBatchOutputsResponse(BaseModel):
    """Custom ListBatchOutputsResponse that uses flexible BatchOutputInfo."""

    outputs: list[BatchOutputInfo]


class BatchDetails(BaseModel):
    """Custom BatchDetails that accepts flexible result format."""

    batchId: Annotated[UUID, Field(description="The batch ID")]
    createdAt: str | None = None
    flowId: Annotated[str, Field(description="The flow ID")]
    maxConcurrency: Annotated[int | None, Field(ge=0)] = None
    statistics: BatchStatistics | None = None
    status: Annotated[BatchStatus, Field(description="The batch status")]
    totalRuns: Annotated[int, Field(ge=0)]
    updatedAt: str | None = None


# Add to_dict and from_dict methods to all pydantic models for compatibility
def _add_compatibility_methods():
    """Add to_dict/from_dict methods to all exported models."""
    from pydantic import BaseModel

    module = sys.modules[__name__]

    for name in __all__:
        cls = getattr(module, name, None)
        if cls is None or not isinstance(cls, type):
            continue
        if not issubclass(cls, BaseModel):
            continue

        # Add to_dict method
        if not hasattr(cls, "to_dict"):

            def make_to_dict(c: type) -> Any:
                def to_dict(self: Any) -> dict[str, Any]:
                    return self.model_dump(mode="json", by_alias=True, exclude_unset=True)

                return to_dict

            cls.to_dict = make_to_dict(cls)

        # Add from_dict class method
        if not hasattr(cls, "from_dict"):

            def make_from_dict(c: type) -> Any:
                @classmethod
                def from_dict(cls: type, data: dict[str, Any]) -> Any:
                    return cls.model_validate(data)

                return from_dict

            cls.from_dict = make_from_dict(cls)


_add_compatibility_methods()
