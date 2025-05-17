from msgspec import Struct, Raw
from uuid import UUID

class Incoming(Struct, kw_only=True):
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

    method: str
    """
    The method to execute.
    """

    params: Raw
    """
    The parameters to pass to the method.
    """


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

    data: any
    """
    The error data.
    """


class MethodResponse(Struct, kw_only=True):
    """
    Message sent in response to a method request.
    """

    jsonrpc: str = "2.0"
    """
    The JSON-RPC version (must be "2.0")
    """

    id: UUID
    """
    The request id.
    """
    
    result: Raw | None = None
    """
    The result of the method execution.
    """

    error: RemoteError | None = None
    """
    The error that occurred during the method execution.
    """