"""HTTP client for Stepflow API."""

from __future__ import annotations

import httpx


class Client:
    """HTTP client wrapper for Stepflow API."""

    def __init__(
        self,
        base_url: str,
        *,
        timeout: httpx.Timeout | None = None,
        headers: dict[str, str] | None = None,
        raise_on_unexpected_status: bool = True,
    ):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout or httpx.Timeout(300.0)
        self.headers = headers or {}
        self.raise_on_unexpected_status = raise_on_unexpected_status
        self._async_client: httpx.AsyncClient | None = None
        self._sync_client: httpx.Client | None = None

    def get_httpx_client(self) -> httpx.Client:
        """Get synchronous httpx client."""
        if self._sync_client is None:
            self._sync_client = httpx.Client(
                base_url=self.base_url,
                timeout=self.timeout,
                headers=self.headers,
            )
        return self._sync_client

    def get_async_httpx_client(self) -> httpx.AsyncClient:
        """Get asynchronous httpx client."""
        if self._async_client is None:
            self._async_client = httpx.AsyncClient(
                base_url=self.base_url,
                timeout=self.timeout,
                headers=self.headers,
            )
        return self._async_client

    async def aclose(self) -> None:
        """Close async client."""
        if self._async_client is not None:
            await self._async_client.aclose()
            self._async_client = None

    def close(self) -> None:
        """Close sync client."""
        if self._sync_client is not None:
            self._sync_client.close()
            self._sync_client = None


# Alias for backwards compatibility
AuthenticatedClient = Client
