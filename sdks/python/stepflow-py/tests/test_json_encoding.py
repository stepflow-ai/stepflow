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

import json
from uuid import UUID

import msgspec

from stepflow_py.worker.generated_protocol import (
    Error,
    Initialized,
    InitializeParams,
    InitializeResult,
    Method,
    MethodError,
    MethodRequest,
    MethodSuccess,
    Notification,
)


def test_method_request_default_jsonrpc():
    request = MethodRequest(
        id="test-123",
        method=Method.initialize,
        params=InitializeParams(runtime_protocol_version=1),
    )

    # Encode to JSON
    json_bytes = msgspec.json.encode(request)
    json_dict = json.loads(json_bytes)

    # Verify jsonrpc field is present and correct
    assert "jsonrpc" in json_dict
    assert json_dict["jsonrpc"] == "2.0"
    assert json_dict["id"] == "test-123"
    assert json_dict["method"] == "initialize"


def test_method_success_default_jsonrpc():
    response = MethodSuccess(
        id=str(UUID(int=42)), result=InitializeResult(server_protocol_version=1)
    )

    # Encode to JSON
    json_bytes = msgspec.json.encode(response)
    json_dict = json.loads(json_bytes)

    # Verify jsonrpc field is present and correct
    assert "jsonrpc" in json_dict
    assert json_dict["jsonrpc"] == "2.0"
    assert json_dict["id"] == "00000000-0000-0000-0000-00000000002a"
    assert "result" in json_dict


def test_method_error_default_jsonrpc():
    error_response = MethodError(
        id="error-test", error=Error(code=-32601, message="Method not found")
    )

    # Encode to JSON
    json_bytes = msgspec.json.encode(error_response)
    json_dict = json.loads(json_bytes)

    # Verify jsonrpc field is present and correct
    assert "jsonrpc" in json_dict
    assert json_dict["jsonrpc"] == "2.0"
    assert json_dict["id"] == "error-test"
    assert "error" in json_dict
    assert json_dict["error"]["code"] == -32601


def test_notification_default_jsonrpc():
    notification = Notification(method=Method.initialized, params=Initialized())

    # Encode to JSON
    json_bytes = msgspec.json.encode(notification)
    json_dict = json.loads(json_bytes)

    # Verify jsonrpc field is present and correct
    assert "jsonrpc" in json_dict
    assert json_dict["jsonrpc"] == "2.0"
    assert json_dict["method"] == "initialized"
    assert "params" in json_dict


def test_explicit_jsonrpc_override():
    request = MethodRequest(
        id="explicit-test",
        method=Method.initialize,
        params=InitializeParams(runtime_protocol_version=1),
        jsonrpc="2.0",  # Explicitly set
    )

    # Encode to JSON
    json_bytes = msgspec.json.encode(request)
    json_dict = json.loads(json_bytes)

    # Verify jsonrpc field is present and correct
    assert "jsonrpc" in json_dict
    assert json_dict["jsonrpc"] == "2.0"


def test_round_trip_decoding():
    """Test that messages can be encoded and include jsonrpc field correctly."""
    original_request = MethodRequest(
        id=12345,
        method=Method.components_list,
        params=InitializeParams(runtime_protocol_version=1),
    )

    # Encode to JSON
    json_bytes = msgspec.json.encode(original_request)
    json_dict = json.loads(json_bytes)

    # Verify the JSON has correct structure including jsonrpc
    assert json_dict["id"] == 12345
    assert json_dict["method"] == "components/list"
    assert json_dict["jsonrpc"] == "2.0"  # Should have default value
    assert "params" in json_dict


def test_all_message_types_have_jsonrpc():
    """Test that all JSON-RPC message types include the jsonrpc field when encoded."""
    messages = [
        MethodRequest(
            id="req",
            method=Method.initialize,
            params=InitializeParams(runtime_protocol_version=1),
        ),
        MethodSuccess(id="success", result=InitializeResult(server_protocol_version=1)),
        MethodError(id="error", error=Error(code=-32000, message="Test error")),
        Notification(method=Method.initialized, params=Initialized()),
    ]

    for message in messages:
        json_bytes = msgspec.json.encode(message)
        json_dict = json.loads(json_bytes)

        # Every message should have jsonrpc field
        assert "jsonrpc" in json_dict, f"Missing jsonrpc in {type(message).__name__}"
        assert json_dict["jsonrpc"] == "2.0", (
            f"Wrong jsonrpc value in {type(message).__name__}"
        )
