from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.store_flow_response import StoreFlowResponse
from ...types import Response


def _get_kwargs(
    *,
    flow: dict[str, Any],
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/flows",
    }

    _kwargs["json"] = {"flow": flow}

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | StoreFlowResponse | None:
    if response.status_code == 200:
        response_200 = StoreFlowResponse.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = cast(Any, None)
        return response_400

    if response.status_code == 500:
        response_500 = cast(Any, None)
        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Any | StoreFlowResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    flow: dict[str, Any],
) -> Response[Any | StoreFlowResponse]:
    """Store a flow definition

    Args:
        flow (dict[str, Any]): The flow definition to store

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | StoreFlowResponse]
    """

    kwargs = _get_kwargs(
        flow=flow,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    flow: dict[str, Any],
) -> Any | StoreFlowResponse | None:
    """Store a flow definition

    Args:
        flow (dict[str, Any]): The flow definition to store

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | StoreFlowResponse
    """

    return sync_detailed(
        client=client,
        flow=flow,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    flow: dict[str, Any],
) -> Response[Any | StoreFlowResponse]:
    """Store a flow definition

    Args:
        flow (dict[str, Any]): The flow definition to store

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | StoreFlowResponse]
    """

    kwargs = _get_kwargs(
        flow=flow,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    flow: dict[str, Any],
) -> Any | StoreFlowResponse | None:
    """Store a flow definition

    Args:
        flow (dict[str, Any]): The flow definition to store

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | StoreFlowResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            flow=flow,
        )
    ).parsed
