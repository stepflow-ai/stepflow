from typing import Any
from msgspec import Struct, Raw
from uuid import UUID


class RemoteError(Struct, kw_only=True):
    """
    The error that occurred during the method execution.
    """

    code: int
    """
    The error code.
    """

    message: str
    """
    The error message.
    """

    data: dict[str, Any] = {}
    """
    The error data.
    """

class Message(Struct, kw_only=True):
    """
    Message sent to request a method execution.
    """

    jsonrpc: str = "2.0"
    """
    The JSON-RPC version (must be "2.0")
    """

    id: UUID | None = None
    """
    The request id. If not set, this is a notification.
    """

    method: str | None = None
    """
    The method to execute.
    """

    params: Raw = Raw(b"null")
    """
    The parameters to pass to the method.
    """

    result: Raw = Raw(b"null")
    """
    The result of the method execution.
    """

    error: RemoteError | None = None
    """
    The error that occurred during the method execution.
    """