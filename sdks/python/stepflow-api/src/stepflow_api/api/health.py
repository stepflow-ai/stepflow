"""Health API endpoints."""

from ._base import Endpoint
from ..models import HealthResponse

health_check = Endpoint("GET", "/health", HealthResponse)

__all__ = ["health_check"]
