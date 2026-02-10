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

"""Tests for the streamable HTTP server implementation."""

import asyncio
import json

import msgspec
import pytest
import pytest_asyncio

from stepflow_py.worker.context import StepflowContext
from stepflow_py.worker.generated_protocol import (
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
    ObservabilityContext,
)
from stepflow_py.worker.http_server import create_test_app
from stepflow_py.worker.server import StepflowServer

POST_HEADERS = {
    "content-type": "application/json",
    "accept": "application/json, text/event-stream",
}


class ServerHelper:
    """Helper class for HTTP server testing with live server."""

    def __init__(self, app, server):
        self.app = app
        self.server = server
        self.url = None
        self._httpx_client = None
        self._cleanup_func = None

    async def send_request(
        self, method, component=None, input_data=None, request_id=None, headers=None
    ):
        """Send a request to the live server."""
        request = self._create_request(method, component, input_data, request_id)
        assert self._httpx_client is not None, (
            "Server not started - call _start_live_server() first"
        )
        return await self._httpx_client.post(
            f"{self.url}/",
            json=msgspec.to_builtins(request),
            headers=headers or POST_HEADERS,
        )

    def stream_request(
        self, method, component=None, input_data=None, request_id=None, headers=None
    ):
        """Send a streaming request and return the response stream context manager."""
        request = self._create_request(method, component, input_data, request_id)
        assert self._httpx_client is not None, (
            "Server not started - call _start_live_server() first"
        )
        return self._httpx_client.stream(
            "POST",
            f"{self.url}/",
            json=msgspec.to_builtins(request),
            headers=headers or POST_HEADERS,
        )

    def parse_sse_events(self, response_text):
        """Parse SSE response text and return list of JSON events."""
        events = []
        for line in response_text.strip().split("\n"):
            if line.startswith("data: "):
                try:
                    data = json.loads(line[6:])  # Remove "data: " prefix
                    events.append(data)
                except json.JSONDecodeError:
                    continue
        return events

    async def get_next_sse_event(self, stream_response):
        """Get the next parsed SSE event from a streaming response."""
        async for line in stream_response.aiter_lines():
            if line.startswith("data: "):
                try:
                    return json.loads(line[6:])
                except json.JSONDecodeError:
                    continue
        return None

    def sse_events(self, stream_response):
        """Create an SSE event helper for easier test writing."""
        return SSEEventHelper(stream_response, self)

    async def _start_live_server(self):
        """Start a real uvicorn server for integration tests."""
        import socket

        import httpx
        import uvicorn

        # Find an available port
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("localhost", 0))
            port = s.getsockname()[1]

        # Initialize the server
        self.server.set_initialized(True)

        config = uvicorn.Config(
            app=self.app,
            host="localhost",
            port=port,
            log_level="critical",  # Suppress all logs except critical errors
            lifespan="off",  # Disable lifespan for tests to avoid cleanup issues
            access_log=False,  # Disable access logging
        )
        server = uvicorn.Server(config)
        server_task = asyncio.create_task(server.serve())
        await asyncio.sleep(0.3)  # Wait for server to start

        self.url = f"http://localhost:{port}"
        self._httpx_client = httpx.AsyncClient(timeout=5.0)

        async def cleanup():
            # Close HTTP client first
            if self._httpx_client:
                await self._httpx_client.aclose()

            # Signal server to shutdown
            server.should_exit = True
            if hasattr(server, "force_exit"):
                server.force_exit = True

            # Give server a moment to shutdown gracefully
            await asyncio.sleep(0.1)

            # Cancel server task if still running
            if not server_task.done():
                server_task.cancel()
                # Wait for cancellation to complete, but don't wait too long
                try:
                    await asyncio.wait_for(server_task, timeout=0.2)
                except (TimeoutError, asyncio.CancelledError):
                    # Ignore cleanup timeouts - this is expected during test teardown
                    pass

        self._cleanup_func = cleanup

    async def _cleanup(self):
        """Clean up the live server."""
        if self._cleanup_func:
            await self._cleanup_func()

    def _create_request(self, method, component=None, input_data=None, request_id=None):
        """Create common JSON-RPC requests."""
        if request_id is None:
            request_id = f"{method.value.replace('/', '_')}-test"

        if method == Method.components_list:
            return MethodRequest(
                jsonrpc="2.0",
                id=request_id,
                method=method,
                params=ComponentListParams(),
            )
        elif method == Method.components_info:
            return MethodRequest(
                jsonrpc="2.0",
                id=request_id,
                method=method,
                params=ComponentInfoParams(component=component or "/simple_component"),
            )
        elif method == Method.components_execute:
            return MethodRequest(
                jsonrpc="2.0",
                id=request_id,
                method=method,
                params=ComponentExecuteParams(
                    component=component or "/simple_component",
                    input=input_data or {"message": "test"},
                    attempt=1,
                    observability=ObservabilityContext(
                        trace_id=None,
                        span_id=None,
                        run_id="test-run-id",
                        flow_id="test-flow-id",
                        step_id="test_step",
                    ),
                ),
            )
        else:
            raise ValueError(f"Unsupported method: {method}")


class SSEEventHelper:
    """Helper class for reading SSE events in tests with a clean API."""

    def __init__(self, stream_response, server_helper):
        self.stream_response = stream_response
        self.server_helper = server_helper
        self._line_iterator = None
        self._done = False

    async def next(self):
        """Get the next SSE event, returning parsed JSON data."""
        if self._done:
            return None

        if self._line_iterator is None:
            self._line_iterator = self.stream_response.aiter_lines()

        try:
            async for line in self._line_iterator:
                if line.startswith("data: "):
                    try:
                        return json.loads(line[6:])
                    except json.JSONDecodeError:
                        continue
            # If we reach here, the stream ended
            self._done = True
            return None
        except Exception:
            self._done = True
            return None

    def done(self):
        """Check if the SSE stream is complete."""
        return self._done

    async def post_response(self, request_id, result):
        """Post a JSON-RPC response for bidirectional communication.

        Args:
            request_id: The ID of the request being responded to
            result: The result data to send back

        Raises:
            AssertionError: If the response status indicates an error (with details)
        """
        method_response = {"jsonrpc": "2.0", "id": request_id, "result": result}
        post_response = await self.server_helper._httpx_client.post(
            f"{self.server_helper.url}/",
            json=method_response,
            headers=POST_HEADERS,
        )
        assert post_response.is_success, (
            f"Response failed with {post_response.status_code}: {post_response.text}"
        )


@pytest.fixture(scope="session")
def core_server():
    """Create a core server with simple and context-aware components."""

    # Define test message classes
    class SimpleInput(msgspec.Struct):
        message: str

    class SimpleOutput(msgspec.Struct):
        processed_message: str

    class ContextInput(msgspec.Struct):
        data: str

    class ContextOutput(msgspec.Struct):
        result: str
        blob_id: str

    server = StepflowServer()

    @server.component
    def simple_component(input: SimpleInput) -> SimpleOutput:
        """A simple component that doesn't need Context."""
        return SimpleOutput(processed_message=f"Processed: {input.message}")

    @server.component
    async def context_component(
        input: ContextInput, context: StepflowContext
    ) -> ContextOutput:
        """A component that uses Context for HTTP blob operations."""
        blob_id = await context.put_blob(input.data)
        return ContextOutput(
            result=f"Processed with context: {input.data}",
            blob_id=blob_id,
        )

    # Define message types for bidirectional SSE test
    class BidirectionalInput(msgspec.Struct):
        value: int
        run_id: str

    class BidirectionalOutput(msgspec.Struct):
        result: str
        item_count: int

    @server.component
    async def bidirectional_component(
        input: BidirectionalInput, context: StepflowContext
    ) -> BidirectionalOutput:
        """A component that makes outgoing SSE calls for bidirectional communication.

        This uses the runs/get method to make an outgoing SSE call, which is
        responded to by the test harness.
        """
        # Make an outgoing call via SSE using runs/get
        run_status = await context.get_run(input.run_id)

        return BidirectionalOutput(
            result=run_status.status,
            item_count=run_status.items.total,
        )

    return server


# In-memory blob storage for tests
_test_blob_storage: dict[str, dict] = {}


@pytest.fixture(scope="session")
def http_app(core_server):
    """Create FastAPI app with test components for testing."""
    import hashlib

    from fastapi.responses import JSONResponse

    app = create_test_app(core_server)

    # Add mock blob API endpoints for testing
    @app.post("/blobs")
    async def store_blob(request: dict):
        """Mock blob storage endpoint."""
        data = request.get("data")
        blob_type = request.get("blobType", "data")
        # Generate blob ID from content hash
        content_str = json.dumps(data, sort_keys=True)
        blob_id = hashlib.sha256(content_str.encode()).hexdigest()
        _test_blob_storage[blob_id] = {"data": data, "blobType": blob_type}
        return JSONResponse(content={"blobId": blob_id})

    @app.get("/blobs/{blob_id}")
    async def get_blob(blob_id: str):
        """Mock blob retrieval endpoint."""
        if blob_id not in _test_blob_storage:
            return JSONResponse(content={"error": "Blob not found"}, status_code=404)
        blob = _test_blob_storage[blob_id]
        return JSONResponse(
            content={
                "data": blob["data"],
                "blobType": blob["blobType"],
                "blobId": blob_id,
            }
        )

    return app


@pytest_asyncio.fixture
async def test_server(http_app, core_server):
    """Create a ServerHelper instance with live server automatically started."""
    server = ServerHelper(http_app, core_server)
    await server._start_live_server()
    # Configure blob API URL to point to our mock endpoints
    core_server._blob_api_url = f"{server.url}/blobs"
    try:
        yield server
    finally:
        await server._cleanup()


@pytest.mark.asyncio
async def test_invalid_json_request(test_server):
    """Test that invalid JSON returns 400 with parse error."""
    response = await test_server._httpx_client.post(
        f"{test_server.url}/",
        content="invalid json {",
        headers=POST_HEADERS,
    )

    assert response.status_code == 400
    assert response.headers["content-type"] == "application/json"

    error_data = response.json()
    assert error_data["error"]["code"] == -32600  # Parse error
    assert "JSON is malformed" in error_data["error"]["message"]


@pytest.mark.asyncio
async def test_invalid_message_structure(test_server):
    """Test that invalid message structure returns 400."""
    # Send valid JSON but invalid Message structure
    response = await test_server._httpx_client.post(
        f"{test_server.url}/",
        json={"invalid": "message"},
        headers=POST_HEADERS,
    )

    assert response.status_code == 400
    assert response.headers["content-type"] == "application/json"

    error_data = response.json()
    assert error_data["error"]["code"] == -32600  # Invalid Request


@pytest.mark.asyncio
async def test_notification_handling(test_server):
    """Test that notifications return 202 Accepted."""
    notification = Notification(
        jsonrpc="2.0", method=Method.initialized, params=Initialized()
    )

    response = await test_server._httpx_client.post(
        f"{test_server.url}/",
        json=msgspec.to_builtins(notification),
        headers=POST_HEADERS,
    )

    assert response.status_code == 202
    assert response.text == '""'  # Empty body


@pytest.mark.asyncio
async def test_unknown_method(test_server):
    """Test that unknown methods return protocol error."""
    # Create request with invalid method - this should be a protocol error
    # since the method doesn't match any valid Method enum value
    request_dict = {
        "jsonrpc": "2.0",
        "id": "test-123",
        "method": "unknown_method",
        "params": {},
    }

    response = await test_server._httpx_client.post(
        f"{test_server.url}/",
        json=request_dict,
        headers=POST_HEADERS,
    )

    assert response.status_code == 400
    assert response.headers["content-type"] == "application/json"

    error_data = response.json()
    assert error_data["error"]["code"] == -32600  # Invalid Request


@pytest.mark.asyncio
async def test_method_response_handling(test_server):
    """Test handling method responses without pending request."""
    # Create a method response without a corresponding pending request
    # This is an invalid scenario that should cause a protocol error
    response_msg = MethodSuccess(
        jsonrpc="2.0",
        id="test-response-123",
        result=ComponentExecuteResult(output="test result"),
    )

    response = await test_server._httpx_client.post(
        f"{test_server.url}/",
        json=msgspec.to_builtins(response_msg),
        headers=POST_HEADERS,
    )

    # Should return 400 because MessageDecoder validates pending requests
    assert response.status_code == 400
    error_data = response.json()
    assert error_data["error"]["code"] == -32600  # Invalid Request
    assert "no pending request" in error_data["error"]["message"]


@pytest.mark.asyncio
async def test_method_error_handling(test_server):
    """Test handling method error responses without pending request."""
    # Create a method error without a corresponding pending request
    # This is an invalid scenario that should cause a protocol error
    error_msg = MethodError(
        jsonrpc="2.0",
        id="test-error-456",
        error=Error(code=-32603, message="Test error", data=None),
    )

    response = await test_server._httpx_client.post(
        f"{test_server.url}/",
        json=msgspec.to_builtins(error_msg),
        headers=POST_HEADERS,
    )

    # Should return 400 because MessageDecoder validates pending requests
    assert response.status_code == 400
    error_data = response.json()
    assert error_data["error"]["code"] == -32600  # Invalid Request
    assert "no pending request" in error_data["error"]["message"]


@pytest.mark.asyncio
async def test_components_list(test_server):
    """Test components_list method returns component information."""
    response = await test_server.send_request(Method.components_list)

    assert response.status_code == 200
    assert response.headers["content-type"] == "application/json"

    result = response.json()
    assert result["jsonrpc"] == "2.0"
    assert result["id"] == "components_list-test"
    assert "result" in result

    components = result["result"]["components"]

    expected_components = [
        "/simple_component",
        "/context_component",
        "/bidirectional_component",
        "/udf",
    ]
    try:
        import langchain_core  # noqa: F401

        expected_components.extend(["/langchain/invoke"])
    except ImportError:
        pass

    assert len(components) == len(expected_components)

    # Find our test components
    component_names = [comp["component"] for comp in components]
    for name in expected_components:
        assert name in component_names, f"Expected component {name} not found"


@pytest.mark.asyncio
async def test_components_info(test_server):
    """Test components_info method returns component details."""
    response = await test_server.send_request(
        Method.components_info, component="/simple_component"
    )

    assert response.status_code == 200
    assert response.headers["content-type"] == "application/json"

    result = response.json()
    assert result["jsonrpc"] == "2.0"
    assert result["id"] == "components_info-test"
    assert "result" in result

    info = result["result"]["info"]
    assert info["component"] == "/simple_component"
    assert "input_schema" in info
    assert "output_schema" in info


@pytest.mark.asyncio
async def test_components_info_not_found(test_server):
    """Test components_info with non-existent component."""
    response = await test_server.send_request(
        Method.components_info, component="/nonexistent"
    )

    # The server now returns 200 with JSON-RPC error response
    assert response.status_code == 200
    assert response.headers["content-type"] == "application/json"

    result = response.json()
    assert result["jsonrpc"] == "2.0"
    assert result["id"] == "components_info-test"
    assert "error" in result
    assert "not found" in result["error"]["message"]


@pytest.mark.asyncio
async def test_simple_component_execution(test_server):
    """Test executing a component without Context (direct JSON response)."""
    response = await test_server.send_request(
        Method.components_execute,
        component="/simple_component",
        input_data={"message": "Hello World"},
    )

    assert response.status_code == 200
    assert response.headers["content-type"] == "application/json"

    result = response.json()
    assert result["jsonrpc"] == "2.0"
    assert result["id"] == "components_execute-test"
    assert "result" in result

    output = result["result"]["output"]
    assert output["processed_message"] == "Processed: Hello World"


@pytest.mark.asyncio
async def test_component_execution_not_found(test_server):
    """Test executing non-existent component."""
    response = await test_server.send_request(
        Method.components_execute,
        component="/nonexistent",
        input_data={"message": "test"},
    )

    # The server now returns 200 with JSON-RPC error response
    assert response.status_code == 200
    assert response.headers["content-type"] == "application/json"

    result = response.json()
    assert result["jsonrpc"] == "2.0"
    assert result["id"] == "components_execute-test"
    assert "error" in result
    assert "not found" in result["error"]["message"]


@pytest.mark.asyncio
async def test_context_component_without_streaming(test_server):
    """Test Context component without streaming accept header (406)."""
    response = await test_server.send_request(
        Method.components_execute,
        component="/context_component",
        input_data={"data": "test data"},
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


@pytest.mark.asyncio
async def test_http_blob_api(test_server):
    """Test context component with HTTP blob API.

    This test verifies that components using StepflowContext can store blobs
    via the HTTP Blob API during execution. The blob operations happen over
    HTTP (not SSE), so we only receive the final result via SSE.
    """

    async with test_server.stream_request(
        Method.components_execute,
        component="/context_component",
        input_data={"data": "test blob data"},
    ) as response:
        # Should return streaming response
        assert response.status_code == 200
        assert "text/event-stream" in response.headers.get("content-type", "")

        # Create SSE event helper
        sse_events = test_server.sse_events(response)

        # With HTTP blob API, the component makes HTTP calls directly,
        # so we only receive the final response via SSE (no intermediate blobs/put)
        final_response = await sse_events.next()
        assert final_response is not None, "Should receive final component response"
        assert final_response["jsonrpc"] == "2.0"
        assert final_response["id"] == "components_execute-test"
        assert "result" in final_response
        assert (
            final_response["result"]["output"]["result"]
            == "Processed with context: test blob data"
        )
        # Blob ID should be a SHA-256 hash (64 hex chars)
        blob_id = final_response["result"]["output"]["blob_id"]
        assert len(blob_id) == 64, "Blob ID should be SHA-256 hash"
        assert all(c in "0123456789abcdef" for c in blob_id), "Blob ID should be hex"

        # Verify the blob was stored in our mock storage
        assert blob_id in _test_blob_storage
        assert _test_blob_storage[blob_id]["data"] == "test blob data"

        # Verify the stream is closed (no more events)
        next_event = await sse_events.next()
        assert next_event is None, "Should not receive any more events"


@pytest.mark.asyncio
async def test_bidirectional_sse(test_server):
    """Test bidirectional SSE communication with runs/get method.

    This test verifies that components can make outgoing SSE calls to the
    orchestrator and receive responses. The component calls context.get_run()
    which sends a runs/get request over SSE, and the test responds to it.
    """

    async with test_server.stream_request(
        Method.components_execute,
        component="/bidirectional_component",
        input_data={"value": 42, "run_id": "test-run-for-bidir"},
    ) as response:
        # Should return streaming response
        assert response.status_code == 200
        assert "text/event-stream" in response.headers.get("content-type", "")

        # Create SSE event helper
        sse_events = test_server.sse_events(response)

        # First, we should receive the runs/get request from the component
        runs_get_request = await sse_events.next()
        assert runs_get_request is not None, "Should receive runs/get request"
        assert runs_get_request["jsonrpc"] == "2.0"
        assert runs_get_request["method"] == "runs/get"
        assert "id" in runs_get_request
        assert runs_get_request["params"]["runId"] == "test-run-for-bidir"

        # Send response to the runs/get request (must match RunStatusProtocol schema)
        await sse_events.post_response(
            runs_get_request["id"],
            {
                "runId": "test-run-for-bidir",
                "flowId": "test-flow-id-blob",
                "status": "completed",
                "items": {
                    "total": 5,
                    "completed": 5,
                    "running": 0,
                    "failed": 0,
                    "cancelled": 0,
                },
                "createdAt": "2024-01-15T10:30:00Z",
            },
        )

        # Then we should receive the final component response
        final_response = await sse_events.next()
        assert final_response is not None, "Should receive final component response"
        assert final_response["jsonrpc"] == "2.0"
        assert final_response["id"] == "components_execute-test"
        assert "result" in final_response
        assert final_response["result"]["output"]["result"] == "completed"
        assert final_response["result"]["output"]["item_count"] == 5

        # Verify the stream is closed (no more events)
        next_event = await sse_events.next()
        assert next_event is None, "Should not receive any more events"
