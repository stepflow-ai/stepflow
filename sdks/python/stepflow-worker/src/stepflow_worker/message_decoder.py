# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License. You may obtain a copy of
# the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations under
# the License.

"""Message reading and decoding for the Stepflow Python SDK.

This module handles the two-stage deserialization of JSON-RPC messages,
using RawMessage as an implementation detail for efficient parsing.
"""

from typing import Generic, TypeVar

import msgspec
from msgspec import Raw, Struct

from .exceptions import StepflowProtocolError
from .generated_protocol import (
    ComponentExecuteParams,
    ComponentExecuteResult,
    ComponentInfoParams,
    ComponentInfoResult,
    ComponentListParams,
    Error,
    GetBlobParams,
    GetBlobResult,
    GetRunProtocolParams,
    Initialized,
    InitializeParams,
    InitializeResult,
    JsonRpc,
    ListComponentsResult,
    Message,
    Method,
    MethodError,
    MethodRequest,
    MethodSuccess,
    Notification,
    PutBlobParams,
    PutBlobResult,
    RequestId,
    RunStatusProtocol,
    SubmitRunProtocolParams,
)


class _RawMessage(Struct, omit_defaults=True, kw_only=True):
    """Raw message envelope for initial JSON-RPC deserialization.

    This is an implementation detail used only for initial parsing
    to determine message type, then immediately converted to proper typed Message.
    """

    jsonrpc: JsonRpc = "2.0"
    id: RequestId | None = None  # None for notifications
    method: Method | None = None  # None for responses
    params: Raw | msgspec.UnsetType = msgspec.UNSET  # Raw params for two-stage decode
    result: Raw | msgspec.UnsetType = msgspec.UNSET  # Raw result for two-stage decode
    error: Error | msgspec.UnsetType = msgspec.UNSET  # Error for failed responses


T = TypeVar("T")


class MessageDecoder(Generic[T]):
    """Stateful decoder for JSON-RPC messages with pending request tracking.

    This class handles the two-stage deserialization of JSON-RPC messages,
    maintaining a map of pending requests to properly decode responses with
    the correct result types.
    """

    def __init__(self):
        self._pending_requests: dict[RequestId, tuple[type, T]] = {}

    def register_request(
        self, request_id: RequestId, response_type: type, context: T
    ) -> None:
        """Register a pending request with its expected response type and context.

        Args:
            request_id: The request ID to track
            response_type: The expected type of the result field in the response
            context: Associated context/data to return when the response is decoded
        """
        self._pending_requests[request_id] = (response_type, context)

    def register_request_for_method(
        self, request_id: RequestId, method: Method, context: T
    ) -> None:
        """Register a pending request with response type based on method.

        Args:
            request_id: The request ID to track
            method: The method being called (used to determine expected result type)
            context: Associated context/data to return when the response is decoded
        """
        response_type = _get_result_type_for_method(method)
        self.register_request(request_id, response_type, context)

    def decode(self, message_bytes: bytes) -> tuple[Message, T | None]:
        """Decode JSON-RPC message bytes into a properly typed Message.

        Args:
            message_bytes: Raw JSON bytes of the message

        Returns:
            A tuple of (properly typed Message, associated context from pending request
            or None). The context will be non-None for method responses that had a
            matching pending request.

        Raises:
            StepflowProtocolError: If the message is invalid or malformed
        """
        try:
            # First decode as raw message to determine type
            raw_message = msgspec.json.decode(message_bytes, type=_RawMessage)

            # Convert to proper typed message
            return self._convert_raw_to_typed(raw_message)

        except msgspec.DecodeError as e:
            raise StepflowProtocolError(f"Failed to decode message: {e}") from e

    def _convert_raw_to_typed(
        self, raw_message: _RawMessage
    ) -> tuple[Message, T | None]:
        """Convert a raw message to a properly typed message."""
        message: Message
        if raw_message.id is None:
            # No ID -> this must be a notification, which requires a method and params.
            if raw_message.method is None:
                raise StepflowProtocolError("Notification missing 'method' field")
            if raw_message.params is msgspec.UNSET:
                raise StepflowProtocolError("Notification missing 'params' field")
            params = _decode_params_for_method(raw_message.method, raw_message.params)
            message = Notification(
                jsonrpc=raw_message.jsonrpc,
                method=raw_message.method,
                params=params,
            )
            return (message, None)
        elif raw_message.method is not None:
            # This has an ID and method, so it is a method request.
            if raw_message.params is msgspec.UNSET:
                raise StepflowProtocolError("Method request missing 'params' field")
            params = _decode_params_for_method(raw_message.method, raw_message.params)
            message = MethodRequest(
                jsonrpc=raw_message.jsonrpc,
                id=raw_message.id,
                method=raw_message.method,
                params=params,
            )
            return (message, None)
        elif raw_message.result is not msgspec.UNSET:
            if raw_message.error is not msgspec.UNSET:
                raise StepflowProtocolError(
                    f"Method response for id '{raw_message.id}' "
                    "cannot have both 'result' and 'error' fields"
                )

            result_type, context = self._pending_requests.pop(
                raw_message.id, (None, None)
            )
            if result_type is None:
                raise StepflowProtocolError(
                    f"Response with id '{raw_message.id}' has no pending request"
                )

            message = MethodSuccess(
                jsonrpc=raw_message.jsonrpc,
                id=raw_message.id,
                result=msgspec.json.decode(raw_message.result, type=result_type),
            )
            return (message, context)

        elif raw_message.error is not msgspec.UNSET:
            result_type, context = self._pending_requests.pop(
                raw_message.id, (None, None)
            )
            if result_type is None:
                raise StepflowProtocolError(
                    f"Response with id '{raw_message.id}' has no pending request"
                )

            message = MethodError(
                jsonrpc=raw_message.jsonrpc,
                id=raw_message.id,
                error=raw_message.error,
            )
            return (message, context)
        else:
            raise StepflowProtocolError(
                "Invalid message: must have either 'id' or 'method'"
            )


def _decode_params_for_method(method: Method, params_raw: Raw):
    """Decode parameters based on the method type."""
    if method == Method.initialize:
        return msgspec.json.decode(params_raw, type=InitializeParams)
    elif method == Method.initialized:
        return msgspec.json.decode(params_raw, type=Initialized)
    elif method == Method.components_list:
        return msgspec.json.decode(params_raw, type=ComponentListParams)
    elif method == Method.components_info:
        return msgspec.json.decode(params_raw, type=ComponentInfoParams)
    elif method == Method.components_execute:
        return msgspec.json.decode(params_raw, type=ComponentExecuteParams)
    elif method == Method.blobs_get:
        return msgspec.json.decode(params_raw, type=GetBlobParams)
    elif method == Method.blobs_put:
        return msgspec.json.decode(params_raw, type=PutBlobParams)
    elif method == Method.runs_submit:
        return msgspec.json.decode(params_raw, type=SubmitRunProtocolParams)
    elif method == Method.runs_get:
        return msgspec.json.decode(params_raw, type=GetRunProtocolParams)
    else:
        raise StepflowProtocolError(f"Unknown method: {method.value}")


def _get_result_type_for_method(method: Method) -> type:
    """Get the expected result type for a given method."""
    if method == Method.initialize:
        return InitializeResult
    elif method == Method.components_list:
        return ListComponentsResult
    elif method == Method.components_info:
        return ComponentInfoResult
    elif method == Method.components_execute:
        return ComponentExecuteResult
    elif method == Method.blobs_get:
        return GetBlobResult
    elif method == Method.blobs_put:
        return PutBlobResult
    elif method == Method.runs_submit:
        return RunStatusProtocol
    elif method == Method.runs_get:
        return RunStatusProtocol
    else:
        raise StepflowProtocolError(f"Unknown method: {method.value}")
