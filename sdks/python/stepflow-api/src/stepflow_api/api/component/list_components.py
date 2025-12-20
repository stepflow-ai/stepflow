from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.list_components_response import ListComponentsResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    include_schemas: bool | Unset = UNSET,
) -> dict[str, Any]:
    params: dict[str, Any] = {}

    params["includeSchemas"] = include_schemas

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/components",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ListComponentsResponse | None:
    if response.status_code == 200:
        response_200 = ListComponentsResponse.from_dict(response.json())

        return response_200

    if response.status_code == 500:
        response_500 = cast(Any, None)
        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Any | ListComponentsResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    include_schemas: bool | Unset = UNSET,
) -> Response[Any | ListComponentsResponse]:
    """List all available components from plugins

    Args:
        include_schemas (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ListComponentsResponse]
    """

    kwargs = _get_kwargs(
        include_schemas=include_schemas,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    include_schemas: bool | Unset = UNSET,
) -> Any | ListComponentsResponse | None:
    """List all available components from plugins

    Args:
        include_schemas (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ListComponentsResponse
    """

    return sync_detailed(
        client=client,
        include_schemas=include_schemas,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    include_schemas: bool | Unset = UNSET,
) -> Response[Any | ListComponentsResponse]:
    """List all available components from plugins

    Args:
        include_schemas (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ListComponentsResponse]
    """

    kwargs = _get_kwargs(
        include_schemas=include_schemas,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    include_schemas: bool | Unset = UNSET,
) -> Any | ListComponentsResponse | None:
    """List all available components from plugins

    Args:
        include_schemas (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ListComponentsResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            include_schemas=include_schemas,
        )
    ).parsed
