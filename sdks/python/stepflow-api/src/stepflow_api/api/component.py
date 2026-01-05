"""Component API endpoints."""

from ._base import Endpoint
from ..models import ListComponentsResponse

list_components = Endpoint(
    "GET", "/components", ListComponentsResponse, query_params=["include_schemas"]
)

__all__ = ["list_components"]
