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

"""Tests for the Stepflow STDIO transport layer.

This test file focuses on testing the STDIO transport wrapper functionality:
- Basic initialization and delegation
- Stream dependency injection capability
- Transport-specific behavior only (core server logic is tested in test_server.py)
"""

import asyncio

import msgspec
import pytest

from stepflow_py.generated_protocol import (
    InitializeParams,
    Method,
    MethodRequest,
)
from stepflow_py.stdio_server import StepflowStdioServer


@pytest.fixture
def server():
    return StepflowStdioServer()


def test_component_registration_delegation(server):
    """Test that component registration is properly delegated to core server."""

    @server.component
    def test_component(input_data: dict) -> dict:
        return {"result": "test"}

    # Verify delegation works
    assert server.get_component("/test_component") is not None
    assert "/test_component" in server._server.get_components()


@pytest.mark.asyncio
async def test_initialize_flow(server, capsys):
    """Test that the server responds correctly to an initialize request."""
    server._server.set_initialized(False)

    request = MethodRequest(
        jsonrpc="2.0",
        id="init-req",
        method=Method.initialize,
        params=InitializeParams(runtime_protocol_version=1),
    )

    stdin_reader = asyncio.StreamReader()
    stdout_writer = MockStreamWriter()
    request_bytes = msgspec.json.encode(request) + b"\n"
    stdin_reader.feed_data(request_bytes)

    server_task = asyncio.create_task(
        server.start(stdin=stdin_reader, stdout=stdout_writer)
    )
    await asyncio.sleep(0.1)
    stdin_reader.feed_eof()

    try:
        await asyncio.wait_for(server_task, timeout=1.0)
    except TimeoutError:
        server_task.cancel()
        await asyncio.gather(server_task, return_exceptions=True)

    captured = capsys.readouterr()
    assert len(captured.out) > 0
    response_data = msgspec.json.decode(captured.out.strip())
    assert response_data["id"] == "init-req"
    assert response_data["jsonrpc"] == "2.0"


@pytest.mark.asyncio
async def test_no_response_to_initialized_notification(server, capsys):
    """Test that the server does not respond to the 'initialized' notification."""
    server._server.set_initialized(True)

    # Notification: no id field
    notification = {
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {},
    }
    stdin_reader = asyncio.StreamReader()
    stdout_writer = MockStreamWriter()
    notification_bytes = msgspec.json.encode(notification) + b"\n"
    stdin_reader.feed_data(notification_bytes)

    server_task = asyncio.create_task(
        server.start(stdin=stdin_reader, stdout=stdout_writer)
    )
    await asyncio.sleep(0.1)
    stdin_reader.feed_eof()

    try:
        await asyncio.wait_for(server_task, timeout=1.0)
    except TimeoutError:
        server_task.cancel()
        await asyncio.gather(server_task, return_exceptions=True)

    captured = capsys.readouterr()
    # Should be no output for notifications
    assert captured.out.strip() == ""


class MockStreamWriter:
    """Simple mock writer for testing."""

    def __init__(self):
        self.data = []

    def write(self, data: bytes):
        self.data.append(data)

    async def drain(self):
        pass


@pytest.mark.asyncio
async def test_stdio(server, capsys):
    """Test that messages are decoded via the channels."""
    server._server.set_initialized(True)

    # Create a simple request
    request = MethodRequest(
        jsonrpc="2.0",
        id="test-injection",
        method=Method.initialize,
        params=InitializeParams(runtime_protocol_version=1),
    )

    # Create mock streams
    stdin_reader = asyncio.StreamReader()
    stdout_writer = MockStreamWriter()
    request_bytes = msgspec.json.encode(request) + b"\n"
    stdin_reader.feed_data(request_bytes)

    # Start server with injected streams
    server_task = asyncio.create_task(
        server.start(stdin=stdin_reader, stdout=stdout_writer)
    )

    # Give server time to process, then signal EOF
    await asyncio.sleep(0.1)
    stdin_reader.feed_eof()

    # Wait for completion
    try:
        await asyncio.wait_for(server_task, timeout=1.0)
    except TimeoutError:
        server_task.cancel()
        await asyncio.gather(server_task, return_exceptions=True)

    # Verify the server processed the injected input
    captured = capsys.readouterr()
    assert len(captured.out) > 0

    # Parse and verify basic response structure
    response_data = msgspec.json.decode(captured.out.strip())
    assert response_data["id"] == "test-injection"
    assert response_data["jsonrpc"] == "2.0"
