"""Flow API endpoints."""

from ._base import Endpoint
from ..models import (
    Flow,
    StoreFlowResponse,
)

# store_flow uses raw dict param (flow=dict) not typed request
store_flow = Endpoint("POST", "/flows", StoreFlowResponse)
get_flow = Endpoint("GET", "/flows/{flow_id}", Flow)
delete_flow = Endpoint("DELETE", "/flows/{flow_id}")

__all__ = [
    "store_flow",
    "get_flow",
    "delete_flow",
]
