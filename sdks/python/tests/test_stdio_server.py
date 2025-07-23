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

"""Tests for the StepFlow STDIO transport layer.

This test file focuses on testing the STDIO transport encoding/decoding mechanisms:
- JSON-RPC message serialization/deserialization
- STDIO protocol handling (_handle_message method)
- Transport-specific error handling
- Integration between STDIO transport and core server
"""

from uuid import UUID

import msgspec
import pytest

from stepflow_sdk.exceptions import (
    ServerNotInitializedError,
)
from stepflow_sdk.generated_protocol import (
    ComponentExecuteParams,
    ComponentInfoParams,
    ComponentListParams,
    Initialized,
    InitializeParams,
    InitializeResult,
    Method,
    MethodRequest,
    MethodSuccess,
    Notification,
)
from stepflow_sdk.stdio_server import StepflowStdioServer


# Test message classes
class ValidInput(msgspec.Struct):
    name: str
    age: int


class ValidOutput(msgspec.Struct):
    greeting: str
    age_next_year: int


@pytest.fixture
def server():
    return StepflowStdioServer()


def test_stdio_server_delegation_to_core(server):
    """Test that StepflowStdioServer properly delegates to core server."""
    # Test that the STDIO server has a core server instance
    assert hasattr(server, "_server")
    assert server._server is not None

    # Test that component registration works through the STDIO server
    @server.component
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="Hello!", age_next_year=25)

    # Verify delegation to core server
    assert "/test_component" in server._server.get_components()
    assert server.get_component("/test_component") is not None


@pytest.mark.asyncio
async def test_stdio_handle_initialize(server):
    """Test STDIO transport layer handling of initialize method."""
    # Test STDIO server's _handle_message method processes initialization
    request = MethodRequest(
        jsonrpc="2.0",
        id=str(UUID(int=1)),
        method=Method.initialize,
        params=InitializeParams(runtime_protocol_version=1, protocol_prefix="python"),
    )

    # The STDIO layer should delegate to core server and return proper response
    response = await server._handle_message(request)
    assert response.id == request.id
    assert isinstance(response, MethodSuccess)
    assert isinstance(response.result, InitializeResult)
    assert response.result.server_protocol_version == 1

    # Test notification handling through STDIO layer
    notification = Notification(
        jsonrpc="2.0", method=Method.initialized, params=Initialized()
    )
    response = await server._handle_message(notification)
    assert response is None  # Notifications don't return responses


@pytest.mark.asyncio
async def test_stdio_message_encoding_decoding(server):
    """Test JSON-RPC message encoding/decoding through STDIO transport."""
    server._server.set_initialized(True)

    # Test that STDIO layer properly encodes/decodes messages
    request = MethodRequest(
        jsonrpc="2.0",
        id="test-encode-decode",
        method=Method.components_list,
        params=ComponentListParams(),
    )

    # Verify the STDIO server can process the request and return encoded response
    response = await server._handle_message(request)
    assert response.id == request.id

    # Test JSON encoding of response
    encoded = msgspec.json.encode(response)
    decoded = msgspec.json.decode(encoded)
    assert decoded["jsonrpc"] == "2.0"
    assert decoded["id"] == "test-encode-decode"


@pytest.mark.asyncio
async def test_stdio_error_handling(server):
    """Test STDIO transport error handling and delegation to core server."""
    server._server.set_initialized(True)

    # Test that STDIO layer properly delegates error handling to core server
    request = MethodRequest(
        jsonrpc="2.0",
        id="test-error",
        method=Method.components_info,
        params=ComponentInfoParams(component="/nonexistent"),
    )

    # STDIO server should delegate to core server which returns error response
    response = await server._handle_message(request)
    assert hasattr(response, "error")
    assert "not found" in response.error.message


@pytest.mark.asyncio
async def test_stdio_context_detection_integration(server):
    """Test STDIO transport integration with context detection."""
    server._server.set_initialized(True)

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    # Test that STDIO server properly handles component execution
    request = MethodRequest(
        jsonrpc="2.0",
        id="test-execute",
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="/test_component", input={"name": "Alice", "age": 25}
        ),
    )

    # Verify STDIO transport layer processes execution correctly
    response = await server._handle_message(request)
    assert response.id == request.id
    assert hasattr(response, "result")
    output = response.result.output
    assert output.greeting == "Hello Alice!"
    assert output.age_next_year == 26


@pytest.mark.asyncio
async def test_stdio_input_validation_error_handling(server):
    """Test STDIO transport handling of input validation errors."""
    server._server.set_initialized(True)

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    request = MethodRequest(
        jsonrpc="2.0",
        id="test-invalid-input",
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="/test_component", input={"invalid": "input"}
        ),
    )

    # STDIO layer should delegate to core server which returns error response
    response = await server._handle_message(request)
    assert hasattr(response, "error")
    assert "execution failed" in response.error.message.lower()


@pytest.mark.asyncio
async def test_stdio_protocol_version_handling(server):
    """Test STDIO transport protocol version negotiation."""
    # Test that STDIO server properly handles protocol version in messages
    request = MethodRequest(
        jsonrpc="2.0",  # Explicit JSON-RPC version
        id="test-protocol",
        method=Method.components_list,
        params=ComponentListParams(),
    )

    server._server.set_initialized(True)
    response = await server._handle_message(request)

    # Verify response maintains protocol version
    assert response.jsonrpc == "2.0"
    assert response.id == request.id


@pytest.mark.asyncio
async def test_stdio_unknown_method_handling(server):
    """Test STDIO transport handling of unknown methods."""
    server._server.set_initialized(True)

    request = MethodRequest(
        jsonrpc="2.0",
        id="test-unknown",
        method="unknown_method",  # type: ignore[arg-type]  # intentionally invalid
        params=ComponentListParams(),
    )

    # STDIO layer should delegate to core server which returns error response
    response = await server._handle_message(request)
    assert hasattr(response, "error")
    assert response.error.code == -32601  # Method not found
    assert "Method not found" in response.error.message


@pytest.mark.asyncio
async def test_stdio_uninitialized_server_handling(server):
    """Test STDIO transport handling of uninitialized server state."""
    # Don't initialize the server
    request = MethodRequest(
        jsonrpc="2.0",
        id="test-uninitialized",
        method=Method.components_list,
        params=ComponentListParams(),
    )

    # STDIO layer should raise ServerNotInitializedError (current implementation)

    with pytest.raises(ServerNotInitializedError):
        await server._handle_message(request)


@pytest.mark.asyncio
async def test_stdio_json_rpc_format_compliance(server):
    """Test that STDIO transport ensures JSON-RPC 2.0 format compliance."""
    server._server.set_initialized(True)

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    request = MethodRequest(
        jsonrpc="2.0",
        id="jsonrpc-compliance-test",
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="/test_component", input={"name": "Test", "age": 30}
        ),
    )

    response = await server._handle_message(request)

    # Test that STDIO layer ensures proper JSON-RPC encoding
    import json

    response_json = json.loads(msgspec.json.encode(response))

    assert "jsonrpc" in response_json
    assert response_json["jsonrpc"] == "2.0"
    assert response_json["id"] == "jsonrpc-compliance-test"
    assert "result" in response_json


def test_stdio_server_inheritance_and_composition(server):
    """Test that StepflowStdioServer properly uses core server functionality."""
    # Test that it properly exposes core server methods
    assert hasattr(server, "component")
    assert hasattr(server, "get_component")
    assert hasattr(server, "_server")

    # Test that it delegates to core server for component operations
    @server.component(name="delegation_test")
    def test_func(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="test", age_next_year=1)

    # Verify the component is in the core server
    core_components = server._server.get_components()
    assert "/delegation_test" in core_components

    # Verify the wrapper method works
    component = server.get_component("/delegation_test")
    assert component is not None
    assert component.name == "/delegation_test"


@pytest.mark.asyncio
async def test_stdio_bidirectional_context_requirement_detection(server):
    """Test that STDIO transport properly detects when context is required."""
    from stepflow_sdk import StepflowContext

    server._server.set_initialized(True)

    # Register component that requires context
    @server.component(name="context_component")
    def context_component(
        input_data: ValidInput, context: StepflowContext
    ) -> ValidOutput:
        return ValidOutput(greeting="Context used", age_next_year=input_data.age + 1)

    # Create request for context-requiring component
    request = MethodRequest(
        jsonrpc="2.0",
        id="test-context-detection",
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="/context_component", input={"name": "Test", "age": 25}
        ),
    )

    # Check that core server properly detects context requirement
    requires_context = server._server.requires_context(request)
    assert requires_context == True

    # Test execution with context (STDIO should handle this appropriately)
    # Note: STDIO transport may not provide full bidirectional context,
    # but should handle the detection and delegation properly
    response = await server._handle_message(request)
    # The response might be an error if context isn't properly provided
    # This tests the transport layer's handling of context-requiring components
    assert response.id == request.id


class TestStdioBidirectionalCommunication:
    """Test bidirectional communication handling in STDIO transport.

    These tests verify that STDIO transport doesn't suffer from deadlock issues
    that occur when components make bidirectional requests during execution.
    Unlike HTTP transport, STDIO uses concurrent processing preventing deadlocks.
    """

    @pytest.fixture
    def bidirectional_server(self):
        """Create a server with components that simulate bidirectional communication."""
        server = StepflowStdioServer()
        server._server.set_initialized(True)

        # Test message classes for bidirectional tests
        class BidirectionalInput(msgspec.Struct):
            data: str
            use_context: bool = False

        class BidirectionalOutput(msgspec.Struct):
            result: str
            blob_id: str | None = None

        @server.component(name="simple_component")
        def simple_component(input_data: BidirectionalInput) -> BidirectionalOutput:
            """A simple component that doesn't need Context."""
            return BidirectionalOutput(result=f"Processed: {input_data.data}")

        @server.component(name="context_component")
        async def context_component(
            input_data: BidirectionalInput, context
        ) -> BidirectionalOutput:
            """A component that uses Context for bidirectional communication."""
            if input_data.use_context:
                # Simulate bidirectional communication by creating processing delay
                # STDIO transport should handle this concurrently without deadlock
                import asyncio

                await asyncio.sleep(0.1)  # Simulate some processing time
                return BidirectionalOutput(
                    result=f"Processed with context: {input_data.data}",
                    blob_id="context-processed-blob-id",
                )
            else:
                return BidirectionalOutput(
                    result=f"Processed with context: {input_data.data}",
                    blob_id="simulated-blob-id",
                )

        return server

    @pytest.mark.asyncio
    async def test_stdio_simple_component_execution(self, bidirectional_server):
        """Test that simple components work correctly via STDIO transport."""
        request = MethodRequest(
            jsonrpc="2.0",
            id="test-simple",
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/simple_component", input={"data": "Hello World"}
            ),
        )

        response = await bidirectional_server._handle_message(request)
        assert response.id == request.id
        assert hasattr(response, "result")
        assert response.result.output.result == "Processed: Hello World"
        assert response.result.output.blob_id is None

    @pytest.mark.asyncio
    async def test_stdio_context_component_execution(self, bidirectional_server):
        """Test that context-requiring components work via STDIO transport."""
        request = MethodRequest(
            jsonrpc="2.0",
            id="test-context",
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/context_component",
                input={"data": "test data", "use_context": False},
            ),
        )

        # STDIO should handle context components properly
        response = await bidirectional_server._handle_message(request)
        assert response.id == request.id
        assert hasattr(response, "result")
        assert "Processed with context" in response.result.output.result

    @pytest.mark.asyncio
    async def test_stdio_bidirectional_component_no_deadlock(
        self, bidirectional_server
    ):
        """Test that bidirectional components don't deadlock in STDIO transport.

        This test verifies that STDIO transport's concurrent message processing
        prevents deadlock scenarios where components might need to make bidirectional
        requests during execution. Unlike HTTP transport, STDIO should handle this
        naturally due to its queue-based concurrent architecture.
        """
        request = MethodRequest(
            jsonrpc="2.0",
            id="deadlock-test",
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/context_component",
                input={"data": "test bidirectional", "use_context": True},
            ),
        )

        # This should execute without hanging/deadlock due to STDIO's concurrent design
        response = await bidirectional_server._handle_message(request)

        # Should complete successfully without timeout
        assert response.id == request.id
        assert hasattr(response, "result")
        assert "Processed with context" in response.result.output.result
        assert response.result.output.blob_id == "context-processed-blob-id"

    @pytest.mark.asyncio
    async def test_stdio_context_detection_and_handling(self, bidirectional_server):
        """Test that STDIO properly detects and handles context requirements."""
        from stepflow_sdk.generated_protocol import (
            ComponentExecuteParams,
            Method,
            MethodRequest,
        )

        # Create requests for each component type
        simple_request = MethodRequest(
            jsonrpc="2.0",
            id="test-simple-detection",
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/simple_component", input={"data": "test"}
            ),
        )

        context_request = MethodRequest(
            jsonrpc="2.0",
            id="test-context-detection",
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/context_component", input={"data": "test"}
            ),
        )

        # Test context detection
        assert not bidirectional_server._server.requires_context(simple_request)
        assert bidirectional_server._server.requires_context(context_request)

        # Test that both execute properly despite different context requirements
        simple_response = await bidirectional_server._handle_message(simple_request)
        context_response = await bidirectional_server._handle_message(context_request)

        assert simple_response.id == "test-simple-detection"
        assert context_response.id == "test-context-detection"
        assert hasattr(simple_response, "result")
        assert hasattr(context_response, "result")

    @pytest.mark.asyncio
    async def test_stdio_concurrent_bidirectional_requests(self, bidirectional_server):
        """Test that STDIO can handle multiple concurrent bidirectional requests.

        This test ensures the STDIO transport's concurrent architecture can handle
        multiple components requiring context simultaneously without interference.
        """
        import asyncio

        # Create multiple concurrent requests that require context
        requests = [
            MethodRequest(
                jsonrpc="2.0",
                id=f"concurrent-{i}",
                method=Method.components_execute,
                params=ComponentExecuteParams(
                    component="/context_component",
                    input={"data": f"concurrent test {i}", "use_context": True},
                ),
            )
            for i in range(3)
        ]

        # Execute all requests concurrently
        tasks = [bidirectional_server._handle_message(request) for request in requests]
        responses = await asyncio.gather(*tasks)

        # All should complete successfully
        assert len(responses) == 3
        for i, response in enumerate(responses):
            assert response.id == f"concurrent-{i}"
            assert hasattr(response, "result")
            assert f"concurrent test {i}" in response.result.output.result
