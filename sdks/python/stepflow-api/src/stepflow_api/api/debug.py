"""Debug API endpoints."""

from ._base import Endpoint
from ..models import (
    DebugStepRequest,
    DebugStepResponse,
    DebugRunnableResponse,
)

debug_continue = Endpoint("POST", "/debug/{run_id}/continue", DebugStepResponse, DebugStepRequest)
debug_execute_step = Endpoint("POST", "/debug/{run_id}/step", DebugStepResponse, DebugStepRequest)
debug_get_runnable = Endpoint("GET", "/debug/{run_id}/runnable", DebugRunnableResponse)

__all__ = [
    "debug_continue",
    "debug_execute_step",
    "debug_get_runnable",
]
