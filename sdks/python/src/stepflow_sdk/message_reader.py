# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

"""
Message reading and decoding for the StepFlow Python SDK.

This module handles the two-stage deserialization of JSON-RPC messages,
using RawMessage as an implementation detail for efficient parsing.
"""

import msgspec
from msgspec import Raw, Struct

from .generated_protocol import (
    Message,
    MethodRequest,
    MethodResponse,
    MethodSuccess,
    MethodError,
    Notification,
    Method,
    RequestId,
    Error,
)
from .exceptions import StepflowProtocolError


class _RawMessage(Struct, kw_only=True):
    """
    Raw message envelope for initial JSON-RPC deserialization.

    This is an implementation detail used only for initial parsing
    to determine message type, then immediately converted to proper typed Message.
    """

    jsonrpc: str = "2.0"
    id: RequestId | None = None  # None for notifications
    method: str | None = None  # None for responses
    params: Raw | None = None  # Raw params for two-stage decode
    result: Raw | None = None  # Raw result for two-stage decode
    error: Error | None = None  # Error for failed responses


def decode_message(message_bytes: bytes) -> Message:
    """
    Decode JSON-RPC message bytes into a properly typed Message.

    Args:
        message_bytes: Raw JSON bytes of the message

    Returns:
        A properly typed Message (MethodRequest, MethodResponse, or Notification)

    Raises:
        StepflowProtocolError: If the message is invalid or malformed
    """
    try:
        # First decode as raw message to determine type
        raw_message = msgspec.json.decode(message_bytes, type=_RawMessage)

        # Convert to proper typed message
        return _convert_raw_to_typed(raw_message)

    except msgspec.DecodeError as e:
        raise StepflowProtocolError(f"Failed to decode message: {e}")


def _convert_raw_to_typed(raw_message: _RawMessage) -> Message:
    """Convert a raw message to a properly typed message."""
    # Determine message type based on presence of id/method
    if raw_message.id is not None and raw_message.method is not None:
        # Method request
        return MethodRequest(
            jsonrpc=raw_message.jsonrpc,
            id=raw_message.id,
            method=Method(raw_message.method),
            params=msgspec.json.decode(raw_message.params),
        )
    elif raw_message.id is not None and raw_message.method is None:
        # Method response
        if raw_message.error is not None:
            return MethodError(
                jsonrpc=raw_message.jsonrpc, id=raw_message.id, error=raw_message.error
            )
        else:
            return MethodSuccess(
                jsonrpc=raw_message.jsonrpc,
                id=raw_message.id,
                result=msgspec.json.decode(raw_message.result),
            )
    elif raw_message.method is not None and raw_message.id is None:
        # Notification
        return Notification(
            jsonrpc=raw_message.jsonrpc,
            method=Method(raw_message.method),
            params=msgspec.json.decode(raw_message.params),
        )
    else:
        raise StepflowProtocolError("Invalid message: no id or method")
