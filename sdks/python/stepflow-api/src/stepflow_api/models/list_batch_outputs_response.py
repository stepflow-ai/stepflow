"""Custom ListBatchOutputsResponse that uses flexible BatchOutputInfo."""

from pydantic import BaseModel

from .batch_output_info import BatchOutputInfo


class ListBatchOutputsResponse(BaseModel):
    """Custom ListBatchOutputsResponse that uses flexible BatchOutputInfo."""

    outputs: list[BatchOutputInfo]


__all__ = ["ListBatchOutputsResponse"]
