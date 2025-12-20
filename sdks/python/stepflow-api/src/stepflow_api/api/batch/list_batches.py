from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.batch_status import BatchStatus
from ...models.list_batches_response import ListBatchesResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    status: BatchStatus | None | Unset = UNSET,
    flow_name: None | str | Unset = UNSET,
    limit: int | None | Unset = UNSET,
    offset: int | None | Unset = UNSET,
) -> dict[str, Any]:
    params: dict[str, Any] = {}

    json_status: None | str | Unset
    if isinstance(status, Unset):
        json_status = UNSET
    elif isinstance(status, BatchStatus):
        json_status = status.value
    else:
        json_status = status
    params["status"] = json_status

    json_flow_name: None | str | Unset
    if isinstance(flow_name, Unset):
        json_flow_name = UNSET
    else:
        json_flow_name = flow_name
    params["flowName"] = json_flow_name

    json_limit: int | None | Unset
    if isinstance(limit, Unset):
        json_limit = UNSET
    else:
        json_limit = limit
    params["limit"] = json_limit

    json_offset: int | None | Unset
    if isinstance(offset, Unset):
        json_offset = UNSET
    else:
        json_offset = offset
    params["offset"] = json_offset

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/batches",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ListBatchesResponse | None:
    if response.status_code == 200:
        response_200 = ListBatchesResponse.from_dict(response.json())

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
) -> Response[Any | ListBatchesResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    status: BatchStatus | None | Unset = UNSET,
    flow_name: None | str | Unset = UNSET,
    limit: int | None | Unset = UNSET,
    offset: int | None | Unset = UNSET,
) -> Response[Any | ListBatchesResponse]:
    """List batches with optional filtering

    Args:
        status (BatchStatus | None | Unset):
        flow_name (None | str | Unset):
        limit (int | None | Unset):
        offset (int | None | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ListBatchesResponse]
    """

    kwargs = _get_kwargs(
        status=status,
        flow_name=flow_name,
        limit=limit,
        offset=offset,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    status: BatchStatus | None | Unset = UNSET,
    flow_name: None | str | Unset = UNSET,
    limit: int | None | Unset = UNSET,
    offset: int | None | Unset = UNSET,
) -> Any | ListBatchesResponse | None:
    """List batches with optional filtering

    Args:
        status (BatchStatus | None | Unset):
        flow_name (None | str | Unset):
        limit (int | None | Unset):
        offset (int | None | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ListBatchesResponse
    """

    return sync_detailed(
        client=client,
        status=status,
        flow_name=flow_name,
        limit=limit,
        offset=offset,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    status: BatchStatus | None | Unset = UNSET,
    flow_name: None | str | Unset = UNSET,
    limit: int | None | Unset = UNSET,
    offset: int | None | Unset = UNSET,
) -> Response[Any | ListBatchesResponse]:
    """List batches with optional filtering

    Args:
        status (BatchStatus | None | Unset):
        flow_name (None | str | Unset):
        limit (int | None | Unset):
        offset (int | None | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ListBatchesResponse]
    """

    kwargs = _get_kwargs(
        status=status,
        flow_name=flow_name,
        limit=limit,
        offset=offset,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    status: BatchStatus | None | Unset = UNSET,
    flow_name: None | str | Unset = UNSET,
    limit: int | None | Unset = UNSET,
    offset: int | None | Unset = UNSET,
) -> Any | ListBatchesResponse | None:
    """List batches with optional filtering

    Args:
        status (BatchStatus | None | Unset):
        flow_name (None | str | Unset):
        limit (int | None | Unset):
        offset (int | None | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ListBatchesResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            status=status,
            flow_name=flow_name,
            limit=limit,
            offset=offset,
        )
    ).parsed
