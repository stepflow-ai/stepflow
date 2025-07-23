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

"""Tests for the streamable HTTP server implementation."""

import asyncio
import json
import uuid

import msgspec
import pytest
from fastapi.testclient import TestClient

from stepflow_sdk.context import StepflowContext
from stepflow_sdk.generated_protocol import (
    ComponentExecuteParams,
    ComponentExecuteResult,
    ComponentInfoParams,
    ComponentListParams,
    Error,
    Initialized,
    Method,
    MethodError,
    MethodRequest,
    MethodSuccess,
    Notification,
)
from stepflow_sdk.http_server import StepflowStreamableHttpServer
from stepflow_sdk.server import StepflowServer

POST_HEADERS = {
    "content-type": "application/json",
    "accept": "application/json, text/event-stream",
}


@pytest.fixture
def test_server():
    """Create a test server with simple and context-aware components."""

    # Define test message classes
    class SimpleInput(msgspec.Struct):
        message: str

    class SimpleOutput(msgspec.Struct):
        processed_message: str

    class ContextInput(msgspec.Struct):
        data: str
        use_context: bool = False

    class ContextOutput(msgspec.Struct):
        result: str
        blob_id: str | None = None

    server = StepflowServer()

    @server.component
    def simple_component(input: SimpleInput) -> SimpleOutput:
        """A simple component that doesn't need Context."""
        return SimpleOutput(processed_message=f"Processed: {input.message}")

    @server.component
    async def context_component(
        input: ContextInput, context: StepflowContext
    ) -> ContextOutput:
        """A component that uses Context for bidirectional communication."""

        if input.use_context:
            blob_id = await context.put_blob(input.data)
            return ContextOutput(
                result=f"Processed with context: {input.data}",
                blob_id=blob_id,
            )
        else:
            # Simulate context usage without actually using it
            return ContextOutput(
                result=f"Processed with context: {input.data}",
                blob_id="simulated-blob-id",
            )

    return server


@pytest.fixture
def http_server(test_server):
    """Create streamable HTTP server with test components."""
    return StepflowStreamableHttpServer(server=test_server)


@pytest.fixture
def client(http_server):
    """Create test client."""
    return TestClient(http_server.app)


class TestProtocolHandling:
    """Test JSON-RPC protocol handling and message validation."""

    def test_invalid_json_request(self, client):
        """Test that invalid JSON returns 400 with parse error."""
        response = client.post(
            "/",
            content="invalid json {",
            headers=POST_HEADERS,
        )

        assert response.status_code == 400
        assert response.headers["content-type"] == "application/json"

        error_data = response.json()
        assert error_data["error"]["code"] == -32700  # Parse error
        assert "Parse error" in error_data["error"]["message"]

    def test_invalid_message_structure(self, client):
        """Test that invalid message structure returns 400."""
        # Send valid JSON but invalid Message structure
        response = client.post(
            "/",
            json={"invalid": "message"},
            headers=POST_HEADERS,
        )

        assert response.status_code == 400
        assert response.headers["content-type"] == "application/json"

        error_data = response.json()
        assert error_data["error"]["code"] == -32600  # Invalid Request

    def test_notification_handling(self, client):
        """Test that notifications return 202 Accepted."""
        notification = Notification(
            jsonrpc="2.0", method=Method.initialized, params=Initialized()
        )

        response = client.post(
            "/", json=msgspec.to_builtins(notification), headers=POST_HEADERS
        )

        assert response.status_code == 202
        assert response.text == '""'  # Empty body

    def test_unknown_method(self, client):
        """Test that unknown methods return protocol error."""
        # Create request with invalid method - this should be a protocol error
        # since the method doesn't match any valid Method enum value
        request_dict = {
            "jsonrpc": "2.0",
            "id": "test-123",
            "method": "unknown_method",
            "params": {},
        }

        response = client.post(
            "/",
            json=request_dict,
            headers=POST_HEADERS,
        )

        assert response.status_code == 400
        assert response.headers["content-type"] == "application/json"

        error_data = response.json()
        assert error_data["error"]["code"] == -32600  # Invalid Request

    def test_method_response_handling(self, client):
        """Test handling method responses without pending request."""
        # Create a method response without a corresponding pending request
        # This is an invalid scenario that should cause a server error
        response_msg = MethodSuccess(
            jsonrpc="2.0",
            id="test-response-123",
            result=ComponentExecuteResult(output="test result"),
        )

        response = client.post(
            "/", json=msgspec.to_builtins(response_msg), headers=POST_HEADERS
        )

        # Should return 500 because assertions fail for unsolicited responses
        assert response.status_code == 500

    def test_method_error_handling(self, client):
        """Test handling method error responses without pending request."""
        # Create a method error without a corresponding pending request
        # This is an invalid scenario that should cause a server error
        error_msg = MethodError(
            jsonrpc="2.0",
            id="test-error-456",
            error=Error(code=-32603, message="Test error", data=None),
        )

        response = client.post(
            "/",
            json=msgspec.to_builtins(error_msg),
            headers=POST_HEADERS,
        )

        # Should return 500 because assertions fail for unsolicited responses
        assert response.status_code == 500


class TestComponentOperations:
    """Test component listing, info, and execution operations."""

    def test_components_list(self, client):
        """Test components_list method returns component information."""
        request = MethodRequest(
            jsonrpc="2.0",
            id="list-123",
            method=Method.components_list,
            params=ComponentListParams(),
        )

        response = client.post(
            "/",
            json=msgspec.to_builtins(request),
            headers=POST_HEADERS,
        )

        assert response.status_code == 200
        assert response.headers["content-type"] == "application/json"

        result = response.json()
        assert result["jsonrpc"] == "2.0"
        assert result["id"] == "list-123"
        assert "result" in result

        components = result["result"]["components"]
        assert len(components) == 2

        # Find our test components
        component_names = [comp["component"] for comp in components]
        assert "/simple_component" in component_names
        assert "/context_component" in component_names

    def test_components_info(self, client):
        """Test components_info method returns component details."""
        request = MethodRequest(
            jsonrpc="2.0",
            id="info-123",
            method=Method.components_info,
            params=ComponentInfoParams(component="/simple_component"),
        )

        response = client.post(
            "/", json=msgspec.to_builtins(request), headers=POST_HEADERS
        )

        assert response.status_code == 200
        assert response.headers["content-type"] == "application/json"

        result = response.json()
        assert result["jsonrpc"] == "2.0"
        assert result["id"] == "info-123"
        assert "result" in result

        info = result["result"]["info"]
        assert info["component"] == "/simple_component"
        assert "input_schema" in info
        assert "output_schema" in info

    def test_components_info_not_found(self, client):
        """Test components_info with non-existent component."""
        request = MethodRequest(
            jsonrpc="2.0",
            id="info-404",
            method=Method.components_info,
            params=ComponentInfoParams(component="/nonexistent"),
        )

        response = client.post(
            "/",
            json=msgspec.to_builtins(request),
            headers=POST_HEADERS,
        )

        # The server now returns 200 with JSON-RPC error response
        assert response.status_code == 200
        assert response.headers["content-type"] == "application/json"

        result = response.json()
        assert result["jsonrpc"] == "2.0"
        assert result["id"] == "info-404"
        assert "error" in result
        assert "not found" in result["error"]["message"]

    def test_simple_component_execution(self, client):
        """Test executing a component without Context (direct JSON response)."""
        request = MethodRequest(
            jsonrpc="2.0",
            id="exec-simple",
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/simple_component", input={"message": "Hello World"}
            ),
        )

        response = client.post(
            "/",
            json=msgspec.to_builtins(request),
            headers=POST_HEADERS,
        )

        assert response.status_code == 200
        assert response.headers["content-type"] == "application/json"

        result = response.json()
        assert result["jsonrpc"] == "2.0"
        assert result["id"] == "exec-simple"
        assert "result" in result

        output = result["result"]["output"]
        assert output["processed_message"] == "Processed: Hello World"

    def test_component_execution_not_found(self, client):
        """Test executing non-existent component."""
        request = MethodRequest(
            jsonrpc="2.0",
            id="exec-404",
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/nonexistent", input={"message": "test"}
            ),
        )

        response = client.post(
            "/", json=msgspec.to_builtins(request), headers=POST_HEADERS
        )

        # The server now returns 200 with JSON-RPC error response
        assert response.status_code == 200
        assert response.headers["content-type"] == "application/json"

        result = response.json()
        assert result["jsonrpc"] == "2.0"
        assert result["id"] == "exec-404"
        assert "error" in result
        assert "not found" in result["error"]["message"]


class TestContextComponents:
    """Test components that require StepflowContext for bidirectional communication."""

    def test_context_component_without_streaming(self, client):
        """Test Context component without streaming accept header (406)."""
        request = MethodRequest(
            jsonrpc="2.0",
            id="exec-context-no-stream",
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/context_component", input={"data": "test data"}
            ),
        )

        response = client.post(
            "/",
            json=msgspec.to_builtins(request),
            headers={
                "content-type": "application/json",
                "accept": "application/json",  # No streaming accept
            },
        )

        # Should return 406 Not Acceptable because component needs Context
        # but client doesn't accept streaming responses
        assert response.status_code == 406
        assert response.headers["content-type"] == "application/json"

        error_response = response.json()
        assert (
            "Accept header must include application/json and text/event-stream"
            in error_response["error"]
        )

    def test_message_decoder_future_registration_timing(self, http_server):
        """Test that contexts are registered and resolved via decode method."""
        message_decoder = http_server.message_decoder

        # Should start with no pending requests
        assert len(message_decoder._pending_requests) == 0

        # Manually register a request to test the mechanism
        test_context = {"test": "data"}
        message_decoder.register_request(
            "test-123", ComponentExecuteResult, test_context
        )

        # Should now have one pending request
        assert len(message_decoder._pending_requests) == 1
        assert "test-123" in message_decoder._pending_requests

        # Create a response message as bytes and decode it
        response_dict = {
            "jsonrpc": "2.0",
            "id": "test-123",
            "result": {"output": "data"},
        }
        response_bytes = msgspec.json.encode(response_dict)

        # Decode should resolve the pending request and return context
        message, resolved_context = message_decoder.decode(response_bytes)

        # Should be resolved and removed from pending
        assert resolved_context == test_context
        assert len(message_decoder._pending_requests) == 0
        assert isinstance(message, MethodSuccess)
        assert message.id == "test-123"

    @pytest.mark.asyncio
    async def test_concurrent_bidirectional_requests_tracking(self, http_server):
        """Test that multiple concurrent bidirectional requests are properly tracked."""
        message_decoder = http_server.message_decoder

        # Create multiple contexts for concurrent requests
        contexts = {}
        request_ids = ["req-1", "req-2", "req-3"]

        for req_id in request_ids:
            test_context = {"request_id": req_id}
            contexts[req_id] = test_context
            message_decoder.register_request(
                req_id, ComponentExecuteResult, test_context
            )

        # All should be registered
        assert len(message_decoder._pending_requests) == 3

        # Resolve them out of order using decode method
        response_data = [
            {"jsonrpc": "2.0", "id": "req-2", "result": {"output": 1}},
            {"jsonrpc": "2.0", "id": "req-1", "result": {"output": 2}},
            {"jsonrpc": "2.0", "id": "req-3", "result": {"output": 3}},
        ]

        for response_dict in response_data:
            response_bytes = msgspec.json.encode(response_dict)
            message, resolved_context = message_decoder.decode(response_bytes)
            assert resolved_context is not None
            assert resolved_context["request_id"] == response_dict["id"]

        # All should be resolved and removed
        assert len(message_decoder._pending_requests) == 0

    @pytest.mark.asyncio
    async def test_bidirectional_request_response_lifecycle(self, http_server):
        """Test the complete lifecycle of bidirectional request-response."""
        import uuid

        import httpx

        # Initialize the server
        http_server.server.set_initialized(True)

        # Create a proper uvicorn server for testing
        import socket

        import uvicorn

        # Find an available port
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("localhost", 0))
            port = s.getsockname()[1]

        config = uvicorn.Config(
            app=http_server.app, host="localhost", port=port, log_level="error"
        )
        server = uvicorn.Server(config)
        server_task = asyncio.create_task(server.serve())
        await asyncio.sleep(0.3)  # Wait for server to start

        try:
            request = MethodRequest(
                jsonrpc="2.0",
                id="lifecycle-test",
                method=Method.components_execute,
                params=ComponentExecuteParams(
                    component="/context_component",
                    input={"data": "lifecycle test", "use_context": True},
                ),
            )

            async with httpx.AsyncClient(timeout=5.0) as client:
                # Send the request with streaming enabled
                async with client.stream(
                    "POST",
                    f"http://localhost:{port}/",
                    json=msgspec.to_builtins(request),
                    headers=POST_HEADERS,
                ) as response:
                    assert response.status_code == 200
                    assert "text/event-stream" in response.headers.get(
                        "content-type", ""
                    )

                    # Track the bidirectional communication lifecycle
                    bidirectional_requests = []
                    final_response = None

                    try:
                        async with asyncio.timeout(3.0):
                            async for line in response.aiter_lines():
                                if line.startswith("data: "):
                                    try:
                                        data = json.loads(line[6:])
                                        print(f"Received SSE data: {data}")

                                        # Track bidirectional requests
                                        if data.get("method") == "blobs/put":
                                            bidirectional_requests.append(data)

                                            # Send response immediately to test timing
                                            blob_response = await client.post(
                                                f"http://localhost:{port}/",
                                                json={
                                                    "jsonrpc": "2.0",
                                                    "id": data["id"],
                                                    "result": {
                                                        "blob_id": str(uuid.uuid4())
                                                    },
                                                },
                                                headers=POST_HEADERS,
                                            )
                                            print(f"Blob: {blob_response.status_code}")
                                            if blob_response.status_code != 202:
                                                response_bytes = (
                                                    await blob_response.aread()
                                                )
                                                print(f"Unexpected: {response_bytes!r}")

                                        # Track final component response
                                        elif data.get("id") == "lifecycle-test":
                                            final_response = data
                                            break

                                    except json.JSONDecodeError:
                                        continue
                    except TimeoutError:
                        pass

                    # Verify the lifecycle worked correctly
                    print(f"Bidirectional requests: {bidirectional_requests}")
                    print(f"Final response: {final_response}")

                    # We should have received at least one bidirectional request
                    assert len(bidirectional_requests) > 0, (
                        "Should have received bidirectional requests"
                    )

                    # Final response should indicate success or diagnostic info
                    assert final_response is not None, (
                        "Should have received final response"
                    )

        finally:
            # Cleanup server
            server.should_exit = True
            if hasattr(server, "force_exit"):
                server.force_exit = True
            server_task.cancel()
            try:
                await asyncio.wait_for(server_task, timeout=1.0)
            except (TimeoutError, asyncio.CancelledError):
                pass

    def test_context_component_with_streaming_accept(self, client):
        """Test Context component with streaming accept header."""
        request = MethodRequest(
            jsonrpc="2.0",
            id="exec-context-stream",
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/context_component", input={"data": "streaming test"}
            ),
        )

        response = client.post(
            "/", json=msgspec.to_builtins(request), headers=POST_HEADERS
        )

        # Should return SSE stream
        assert response.status_code == 200
        assert response.headers["content-type"] == "text/event-stream; charset=utf-8"

        # Parse SSE response
        lines = response.text.strip().split("\n")
        assert len(lines) >= 1  # At least "data: ..." line

        # Find the JSON-RPC response in the SSE stream
        json_line = None
        for line in lines:
            if line.startswith("data: "):
                json_data = line[6:]  # Remove "data: " prefix
                try:
                    data = msgspec.json.decode(json_data.encode())
                    if (
                        isinstance(data, dict)
                        and data.get("id") == "exec-context-stream"
                    ):
                        json_line = data
                        break
                except Exception:
                    continue

        assert json_line is not None
        assert json_line["jsonrpc"] == "2.0"
        assert json_line["id"] == "exec-context-stream"
        assert "result" in json_line

    @pytest.mark.asyncio
    async def test_bidirectional_component_communication(self, http_server):
        """Test that bidirectional components can communicate with runtime via SSE.

        This test verifies that components requiring Context can successfully
        make bidirectional requests concurrently during execution without deadlock.
        """
        import socket

        import httpx

        # Create a proper uvicorn server instance for better shutdown control
        import uvicorn

        # Find an available port
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("localhost", 0))
            port = s.getsockname()[1]

        config = uvicorn.Config(
            app=http_server.app,
            host="localhost",
            port=port,
            log_level="error",  # Reduce log noise
        )
        server = uvicorn.Server(config)

        # Start the server in a background task
        server_task = asyncio.create_task(server.serve())

        # Wait for server to be ready
        await asyncio.sleep(0.3)

        try:
            # Initialize the base server
            http_server.server.set_initialized(True)

            # Create a component that requires bidirectional communication
            request = MethodRequest(
                jsonrpc="2.0",
                id="bidirectional-test",
                method=Method.components_execute,
                params=ComponentExecuteParams(
                    component="/context_component",
                    input={"data": "test bidirectional", "use_context": True},
                ),
            )

            async with httpx.AsyncClient(timeout=5.0) as client:
                # Send the request with streaming enabled
                async with client.stream(
                    "POST",
                    f"http://localhost:{port}/",
                    json=msgspec.to_builtins(request),
                    headers=POST_HEADERS,
                ) as response:
                    # Should return streaming response
                    assert response.status_code == 200
                    assert "text/event-stream" in response.headers.get(
                        "content-type", ""
                    )

                    # Look for bidirectional requests and final component response
                    found_blob_request = False
                    found_final_response = False

                    # Read SSE stream line by line with timeout
                    try:
                        async with asyncio.timeout(3.0):  # Increased timeout
                            async for line in response.aiter_lines():
                                if line.startswith("data: "):
                                    try:
                                        data = json.loads(
                                            line[6:]
                                        )  # Remove "data: " prefix

                                        # Check if blobs/put request (bidirectional)
                                        if (
                                            data.get("method") == "blobs/put"
                                            and "id" in data
                                            and "params" in data
                                        ):
                                            found_blob_request = True
                                            # Send proper response with msgspec
                                            await client.post(
                                                f"http://localhost:{port}/",
                                                json={
                                                    "jsonrpc": "2.0",
                                                    "id": data["id"],
                                                    "result": {
                                                        "blob_id": str(uuid.uuid4())
                                                    },
                                                },
                                                headers=POST_HEADERS,
                                            )

                                        # Check if this is the final success response
                                        elif (
                                            data.get("id") == "bidirectional-test"
                                            and "result" in data
                                        ):
                                            found_final_response = True
                                            break  # Exit once we get the final response

                                        # Check if error response (component failed)
                                        elif (
                                            data.get("id") == "bidirectional-test"
                                            and "error" in data
                                        ):
                                            # Error but proves no deadlock occurred
                                            found_final_response = True
                                            break

                                    except json.JSONDecodeError:
                                        continue
                    except TimeoutError:
                        pass  # Timeout is acceptable

                    # Verify bidirectional communication attempt (no deadlock)
                    assert found_blob_request, (
                        "Should have received blobs/put request (concurrent processing)"
                    )

                    assert found_final_response, (
                        "Should have received final response (successful completion)"
                    )

        finally:
            # Proper server shutdown
            server.should_exit = True
            if hasattr(server, "force_exit"):
                server.force_exit = True
            server_task.cancel()

            # Wait for graceful shutdown with timeout
            try:
                await asyncio.wait_for(server_task, timeout=1.0)
            except (TimeoutError, asyncio.CancelledError):
                # Force cleanup if graceful shutdown fails
                pass


class TestMessageDecoderIntegration:
    """Test integration with MessageDecoder for bidirectional communication."""
