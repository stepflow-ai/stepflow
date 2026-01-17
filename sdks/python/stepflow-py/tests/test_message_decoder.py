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

"""
Tests for message_decoder module to ensure correct decoding of JSON-RPC messages.
"""

import json

import pytest

from stepflow_py.worker.exceptions import StepflowProtocolError
from stepflow_py.worker.generated_protocol import (
    ComponentExecuteResult,
    Initialized,
    InitializeParams,
    InitializeResult,
    Method,
    MethodError,
    MethodRequest,
    MethodSuccess,
    Notification,
)
from stepflow_py.worker.message_decoder import MessageDecoder


class TestMessageDecoder:
    """Test the MessageDecoder class."""

    def test_decode_method_request_initialize(self):
        """Test decoding a valid initialize method request."""
        message_json = {
            "jsonrpc": "2.0",
            "id": "test-123",
            "method": "initialize",
            "params": {"runtime_protocol_version": 1},
        }
        message_bytes = json.dumps(message_json).encode()

        decoder: MessageDecoder[None] = MessageDecoder()
        message, context = decoder.decode(message_bytes)

        assert isinstance(message, MethodRequest)
        assert message.id == "test-123"
        assert message.method == Method.initialize
        assert isinstance(message.params, InitializeParams)
        assert message.params.runtime_protocol_version == 1
        assert message.jsonrpc == "2.0"
        assert context is None  # No context for requests

    def test_decode_method_success_with_context(self):
        """Test decoding a successful response with registered request context."""
        # Register a pending request
        decoder: MessageDecoder[dict[str, str]] = MessageDecoder()
        test_context = {"future": "mock_future", "timestamp": "2023-01-01"}
        decoder.register_request("init-123", InitializeResult, test_context)

        # Decode response
        message_json = {
            "jsonrpc": "2.0",
            "id": "init-123",
            "result": {"server_protocol_version": 1},
        }
        message_bytes = json.dumps(message_json).encode()

        message, context = decoder.decode(message_bytes)

        assert isinstance(message, MethodSuccess)
        assert message.id == "init-123"
        assert isinstance(message.result, InitializeResult)
        assert message.result.server_protocol_version == 1
        assert context == test_context

        # Verify the pending request was removed
        assert len(decoder._pending_requests) == 0

    def test_decode_method_success_without_context(self):
        """Test decoding a successful response without registered request context."""
        message_json = {
            "jsonrpc": "2.0",
            "id": "unknown-123",
            "result": {"server_protocol_version": 1},
        }
        message_bytes = json.dumps(message_json).encode()

        decoder: MessageDecoder[None] = MessageDecoder()
        # Should now raise an error for unsolicited responses
        with pytest.raises(StepflowProtocolError, match="no pending request"):
            decoder.decode(message_bytes)

    def test_register_request_for_method(self):
        """Test automatic result type registration based on method."""
        decoder: MessageDecoder[str] = MessageDecoder()
        test_context = "test_future"

        # Register using method (automatic type detection)
        decoder.register_request_for_method(
            "exec-123", Method.components_execute, test_context
        )

        # Decode response
        message_json = {
            "jsonrpc": "2.0",
            "id": "exec-123",
            "result": {"output": {"greeting": "Hello World!"}},
        }
        message_bytes = json.dumps(message_json).encode()

        message, context = decoder.decode(message_bytes)

        assert isinstance(message, MethodSuccess)
        assert isinstance(message.result, ComponentExecuteResult)
        assert message.result.output == {"greeting": "Hello World!"}
        assert context == test_context

    def test_decode_method_error_with_context(self):
        """Test decoding an error response with registered request context."""
        decoder: MessageDecoder[str] = MessageDecoder()
        test_context = "error_future"
        decoder.register_request("error-req", InitializeResult, test_context)

        message_json = {
            "jsonrpc": "2.0",
            "id": "error-req",
            "error": {
                "code": -32601,
                "message": "Method not found",
                "data": {"extra": "info"},
            },
        }
        message_bytes = json.dumps(message_json).encode()

        message, context = decoder.decode(message_bytes)

        assert isinstance(message, MethodError)
        assert message.id == "error-req"
        assert message.error.code == -32601
        assert message.error.message == "Method not found"
        assert message.error.data == {"extra": "info"}
        assert context == test_context

    def test_decode_notification(self):
        """Test decoding a notification."""
        message_json = {"jsonrpc": "2.0", "method": "initialized", "params": {}}
        message_bytes = json.dumps(message_json).encode()

        decoder: MessageDecoder[None] = MessageDecoder()
        message, context = decoder.decode(message_bytes)

        assert isinstance(message, Notification)
        assert message.method == Method.initialized
        assert isinstance(message.params, Initialized)
        assert context is None

    def test_decode_multiple_messages_with_different_contexts(self):
        """Test decoding multiple messages with different contexts."""
        decoder: MessageDecoder[str] = MessageDecoder()

        # Register multiple pending requests
        decoder.register_request("req-1", InitializeResult, "context-1")
        decoder.register_request("req-2", ComponentExecuteResult, "context-2")

        # Decode first response
        message1_json = {
            "jsonrpc": "2.0",
            "id": "req-1",
            "result": {"server_protocol_version": 1},
        }
        message1, context1 = decoder.decode(json.dumps(message1_json).encode())

        assert isinstance(message1, MethodSuccess)
        assert isinstance(message1.result, InitializeResult)
        assert context1 == "context-1"

        # Decode second response
        message2_json = {
            "jsonrpc": "2.0",
            "id": "req-2",
            "result": {"output": {"result": "success"}},
        }
        message2, context2 = decoder.decode(json.dumps(message2_json).encode())

        assert isinstance(message2, MethodSuccess)
        assert isinstance(message2.result, ComponentExecuteResult)
        assert context2 == "context-2"

        # Verify both requests were removed
        assert len(decoder._pending_requests) == 0


class TestMessageDecoderErrors:
    """Test error handling in MessageDecoder."""

    def test_decode_invalid_json(self):
        """Test decoding invalid JSON bytes."""
        invalid_json = b'{"invalid": json}'

        decoder: MessageDecoder[None] = MessageDecoder()
        with pytest.raises(StepflowProtocolError, match="Failed to decode message"):
            decoder.decode(invalid_json)

    def test_decode_empty_message(self):
        """Test decoding empty message."""
        empty_message = b"{}"

        decoder: MessageDecoder[None] = MessageDecoder()
        with pytest.raises(
            StepflowProtocolError, match="Notification missing 'method' field"
        ):
            decoder.decode(empty_message)

    def test_decode_invalid_method_name(self):
        """Test decoding message with invalid method name."""
        message_json = {
            "jsonrpc": "2.0",
            "id": "test",
            "method": "invalid_method",
            "params": {},
        }
        message_bytes = json.dumps(message_json).encode()

        decoder: MessageDecoder[None] = MessageDecoder()
        with pytest.raises(StepflowProtocolError, match="Failed to decode message"):
            decoder.decode(message_bytes)

    def test_decode_missing_params(self):
        """Test decoding request without params field."""
        message_json = {
            "jsonrpc": "2.0",
            "id": "test",
            "method": "initialize",
            # Missing params field
        }
        message_bytes = json.dumps(message_json).encode()

        decoder: MessageDecoder[None] = MessageDecoder()
        with pytest.raises(
            StepflowProtocolError, match="Method request missing 'params' field"
        ):
            decoder.decode(message_bytes)

    def test_decode_missing_result(self):
        """Test decoding response without result or error field."""
        message_json = {
            "jsonrpc": "2.0",
            "id": "test",
            # Missing result and error fields
        }
        message_bytes = json.dumps(message_json).encode()

        decoder: MessageDecoder[None] = MessageDecoder()
        with pytest.raises(
            StepflowProtocolError,
            match="Invalid message: must have either 'id' or 'method'",
        ):
            decoder.decode(message_bytes)

    def test_decode_response_with_both_result_and_error(self):
        """Test decoding response with both result and error produces error."""
        message_json = {
            "jsonrpc": "2.0",
            "id": "test",
            "result": {"some": "result"},
            "error": {"code": -32603, "message": "Internal error"},
        }
        message_bytes = json.dumps(message_json).encode()

        decoder: MessageDecoder[None] = MessageDecoder()
        # Should now raise an error for invalid message structure
        with pytest.raises(
            StepflowProtocolError, match="cannot have both 'result' and 'error' fields"
        ):
            decoder.decode(message_bytes)
