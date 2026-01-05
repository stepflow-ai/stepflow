"""Batch API endpoints."""

from ._base import Endpoint
from ..models import (
    CreateBatchRequest,
    CreateBatchResponse,
    BatchDetails,
    CancelBatchResponse,
    ListBatchesResponse,
    ListBatchOutputsResponse,
    ListBatchRunsResponse,
)

create_batch = Endpoint("POST", "/batches", CreateBatchResponse, CreateBatchRequest)
get_batch = Endpoint("GET", "/batches/{batch_id}", BatchDetails)
cancel_batch = Endpoint("POST", "/batches/{batch_id}/cancel", CancelBatchResponse)
get_batch_outputs = Endpoint("GET", "/batches/{batch_id}/outputs", ListBatchOutputsResponse)
list_batch_runs = Endpoint("GET", "/batches/{batch_id}/runs", ListBatchRunsResponse)
list_batches = Endpoint(
    "GET", "/batches", ListBatchesResponse, query_params=["status", "flow_name", "limit", "offset"]
)

__all__ = [
    "create_batch",
    "get_batch",
    "cancel_batch",
    "get_batch_outputs",
    "list_batch_runs",
    "list_batches",
]
