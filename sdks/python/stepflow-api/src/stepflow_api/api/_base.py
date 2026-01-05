"""Thin declarative API layer for HTTP endpoints."""

from __future__ import annotations

from dataclasses import dataclass, field
from http import HTTPStatus
from typing import Any, TypeVar, Generic
from urllib.parse import quote

import httpx

from ..client import Client
from ..types import Response, UNSET, Unset

T = TypeVar("T")
RequestT = TypeVar("RequestT")
ResponseT = TypeVar("ResponseT")


@dataclass
class Endpoint(Generic[RequestT, ResponseT]):
    """Declarative endpoint definition.

    Usage:
        get_run = Endpoint("GET", "/runs/{run_id}", RunDetails)
        create_run = Endpoint("POST", "/runs", CreateRunResponse, CreateRunRequest)
        list_batches = Endpoint("GET", "/batches", ListBatchesResponse,
                                query_params=["status", "flow_name", "limit", "offset"])
    """

    method: str
    path: str
    response_type: type[ResponseT] | None = None
    request_type: type[RequestT] | None = None
    query_params: list[str] = field(default_factory=list)

    def _build_url(self, **path_params: Any) -> str:
        """Build URL with path parameters."""
        url = self.path
        for key, value in path_params.items():
            url = url.replace(f"{{{key}}}", quote(str(value), safe=""))
        return url

    def _build_query(self, **query_params: Any) -> dict[str, Any]:
        """Build query parameters, excluding UNSET values."""
        params = {}
        for key in self.query_params:
            value = query_params.get(key, UNSET)
            if isinstance(value, Unset) or value is None:
                continue
            # Handle enum values
            if hasattr(value, "value"):
                value = value.value
            # Convert snake_case to camelCase for API
            api_key = "".join(
                word.capitalize() if i > 0 else word
                for i, word in enumerate(key.split("_"))
            )
            params[api_key] = value
        return params

    def _parse_response(self, response: httpx.Response) -> ResponseT | None:
        """Parse response based on status code."""
        if response.status_code == 200 and self.response_type is not None:
            return self.response_type.from_dict(response.json())  # type: ignore
        return None

    def _prepare_body(self, body: RequestT | None, kwargs: dict[str, Any]) -> dict[str, Any] | None:
        """Prepare request body from typed body or raw kwargs."""
        if body is not None:
            # Typed request object
            if hasattr(body, "to_dict"):
                return body.to_dict()  # type: ignore
            elif hasattr(body, "model_dump"):
                return body.model_dump(mode="json")  # type: ignore
            else:
                return dict(body)  # type: ignore

        # Check for raw dict params (e.g., flow= for store_flow)
        body_params = {k: v for k, v in kwargs.items()
                       if k not in self.query_params and f"{{{k}}}" not in self.path}
        if body_params:
            return body_params

        return None

    def sync_detailed(
        self,
        *,
        client: Client,
        body: RequestT | None = None,
        **kwargs: Any,
    ) -> Response[ResponseT | None]:
        """Execute synchronous request with full response."""
        path_params = {k: v for k, v in kwargs.items() if f"{{{k}}}" in self.path}
        query_params = {k: v for k, v in kwargs.items() if k in self.query_params}

        request_kwargs: dict[str, Any] = {
            "method": self.method,
            "url": self._build_url(**path_params),
        }

        if query_params:
            request_kwargs["params"] = self._build_query(**query_params)

        body_json = self._prepare_body(body, kwargs)
        if body_json is not None:
            request_kwargs["json"] = body_json
            request_kwargs["headers"] = {"Content-Type": "application/json"}

        response = client.get_httpx_client().request(**request_kwargs)

        return Response(
            status_code=HTTPStatus(response.status_code),
            content=response.content,
            headers=dict(response.headers),
            parsed=self._parse_response(response),
        )

    def sync(
        self,
        *,
        client: Client,
        body: RequestT | None = None,
        **kwargs: Any,
    ) -> ResponseT | None:
        """Execute synchronous request, return parsed response."""
        return self.sync_detailed(client=client, body=body, **kwargs).parsed

    async def asyncio_detailed(
        self,
        *,
        client: Client,
        body: RequestT | None = None,
        **kwargs: Any,
    ) -> Response[ResponseT | None]:
        """Execute async request with full response."""
        path_params = {k: v for k, v in kwargs.items() if f"{{{k}}}" in self.path}
        query_params = {k: v for k, v in kwargs.items() if k in self.query_params}

        request_kwargs: dict[str, Any] = {
            "method": self.method,
            "url": self._build_url(**path_params),
        }

        if query_params:
            request_kwargs["params"] = self._build_query(**query_params)

        body_json = self._prepare_body(body, kwargs)
        if body_json is not None:
            request_kwargs["json"] = body_json
            request_kwargs["headers"] = {"Content-Type": "application/json"}

        response = await client.get_async_httpx_client().request(**request_kwargs)

        return Response(
            status_code=HTTPStatus(response.status_code),
            content=response.content,
            headers=dict(response.headers),
            parsed=self._parse_response(response),
        )

    async def asyncio(
        self,
        *,
        client: Client,
        body: RequestT | None = None,
        **kwargs: Any,
    ) -> ResponseT | None:
        """Execute async request, return parsed response."""
        return (await self.asyncio_detailed(client=client, body=body, **kwargs)).parsed
