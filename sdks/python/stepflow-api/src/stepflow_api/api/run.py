"""Run API endpoints."""

from ._base import Endpoint
from ..models import (
    CreateRunRequest,
    CreateRunResponse,
    RunDetails,
    ListRunsResponse,
    ListStepRunsResponse,
    RunFlowResponse,
)

create_run = Endpoint("POST", "/runs", CreateRunResponse, CreateRunRequest)
get_run = Endpoint("GET", "/runs/{run_id}", RunDetails)
delete_run = Endpoint("DELETE", "/runs/{run_id}")
cancel_run = Endpoint("POST", "/runs/{run_id}/cancel", RunDetails)
get_run_steps = Endpoint("GET", "/runs/{run_id}/steps", ListStepRunsResponse)
get_run_flow = Endpoint("GET", "/runs/{run_id}/flow", RunFlowResponse)
list_runs = Endpoint(
    "GET", "/runs", ListRunsResponse, query_params=["status", "flow_id", "limit", "offset"]
)

__all__ = [
    "create_run",
    "get_run",
    "delete_run",
    "cancel_run",
    "get_run_steps",
    "get_run_flow",
    "list_runs",
]
