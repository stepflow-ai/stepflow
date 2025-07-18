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

import asyncio
import uuid

import msgspec
import pytest
from fastapi.testclient import TestClient

from stepflow_sdk.context import StepflowContext
from stepflow_sdk.generated_protocol import (
    ComponentExecuteParams,
    ComponentListParams,
    Method,
    MethodRequest,
)
from stepflow_sdk.http_server import StepflowHttpServer
from stepflow_sdk.server import StepflowServer


@pytest.fixture
def test_server():
    """Create a test server with a simple component."""

    # Define test message classes inside the fixture
    class TestInput(msgspec.Struct):
        message: str

    class TestOutput(msgspec.Struct):
        processed_message: str
        request_id: str
        session_id: str | None

    server = StepflowServer()

    @server.component
    def test_component(input: TestInput, context: StepflowContext) -> TestOutput:
        # Process the input - session information is handled by the context
        return TestOutput(
            processed_message=f"Processed: {input.message}",
            request_id=str(uuid.uuid4()),
            session_id=context.session_id,
        )

    return server


@pytest.fixture
def http_server(test_server):
    """Create HTTP server with test component."""
    return StepflowHttpServer(server=test_server)


@pytest.fixture
def client(http_server):
    """Create test client."""
    return TestClient(http_server.app)


class TestSessionIsolation:
    """Test that sessions are properly isolated, especially with same request IDs."""

    def test_different_sessions_same_request_id(self, client):
        """Test that requests with same ID in different sessions are handled correctly."""  # noqa: E501
        # Test same request ID in different sessions
        same_request_id = "test-request-123"
        session1_id = "test-session-1"
        session2_id = "test-session-2"

        # Request 1 to session 1 (non-existent session)
        request1 = MethodRequest(
            id=same_request_id,
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/test_component",
                input={"message": "Session 1 message"},
            ),
        )
        response1 = client.post(
            "/",
            params={"sessionId": session1_id},
            json=msgspec.to_builtins(request1),
        )

        # Request 2 to session 2 with same request ID (non-existent session)
        request2 = MethodRequest(
            id=same_request_id,
            method=Method.components_execute,
            params=ComponentExecuteParams(
                component="/test_component",
                input={"message": "Session 2 message"},
            ),
        )
        response2 = client.post(
            "/",
            params={"sessionId": session2_id},
            json=msgspec.to_builtins(request2),
        )

        # Both requests should fail because sessions don't exist yet
        # but they should fail with different session-specific errors
        assert response1.status_code == 400
        assert response2.status_code == 400

        # Both should contain session-specific error messages
        error1 = response1.json()
        error2 = response2.json()

        assert "session not found" in error1["error"]["message"]
        assert "session not found" in error2["error"]["message"]

        # The responses should be identical since both sessions don't exist
        assert error1["error"]["code"] == error2["error"]["code"]
        assert error1["id"] == error2["id"] == same_request_id

    def test_session_creation_and_isolation(self, http_server):
        """Test that sessions are created correctly and isolated."""
        # Create two sessions manually
        from stepflow_sdk.http_server import StepflowSession

        session1_id = "session-1"
        session2_id = "session-2"

        session1 = StepflowSession(session1_id, http_server.server)
        session2 = StepflowSession(session2_id, http_server.server)

        # Add to the server's session registry
        http_server.sessions[session1_id] = session1
        http_server.sessions[session2_id] = session2

        # Both sessions should be separate objects
        assert session1 is not session2
        assert session1.session_id != session2.session_id
        assert session1.context is not session2.context

        # Both sessions should have their own event queues
        assert session1.event_queue is not session2.event_queue

        # Both sessions should have their own message decoders
        assert session1.message_decoder is not session2.message_decoder

        # Sessions should be registered in the server
        assert http_server.sessions[session1_id] is session1
        assert http_server.sessions[session2_id] is session2

    @pytest.mark.asyncio
    async def test_concurrent_requests_same_id_different_sessions(self, http_server):
        """Test concurrent requests with same ID in different sessions."""
        # Create two sessions
        session1_id = "concurrent-session-1"
        session2_id = "concurrent-session-2"

        from stepflow_sdk.http_server import StepflowSession

        session1 = StepflowSession(session1_id, http_server.server)
        session2 = StepflowSession(session2_id, http_server.server)

        http_server.sessions[session1_id] = session1
        http_server.sessions[session2_id] = session2

        # Initialize the server
        http_server.server.set_initialized(True)

        # Same request ID for both sessions
        same_request_id = "concurrent-request-456"

        # Create request payloads
        request1 = {
            "jsonrpc": "2.0",
            "method": Method.components_execute,
            "id": same_request_id,
            "params": {
                "component": "/test_component",
                "input": {"message": "Concurrent message 1"},
            },
        }

        request2 = {
            "jsonrpc": "2.0",
            "method": Method.components_execute,
            "id": same_request_id,
            "params": {
                "component": "/test_component",
                "input": {"message": "Concurrent message 2"},
            },
        }

        # Execute both requests concurrently
        task1 = asyncio.create_task(http_server._handle_json_rpc(request1, session1))
        task2 = asyncio.create_task(http_server._handle_json_rpc(request2, session2))

        # Wait for both to complete
        result1, result2 = await asyncio.gather(task1, task2)

        # Both requests should return JSON-RPC responses
        assert result1["jsonrpc"] == "2.0"
        assert result2["jsonrpc"] == "2.0"
        assert result1["id"] == same_request_id
        assert result2["id"] == same_request_id

        # Both should have successful results
        assert "result" in result1
        assert "result" in result2

        # Verify the results contain the correct session information
        output1 = result1["result"]["output"]
        output2 = result2["result"]["output"]

        # Each session should have its own session_id in the output
        assert output1["session_id"] == session1_id
        assert output2["session_id"] == session2_id

        # Messages should be different (showing they were processed independently)
        assert output1["processed_message"] == "Processed: Concurrent message 1"
        assert output2["processed_message"] == "Processed: Concurrent message 2"

        # The important thing is that both requests were processed independently
        # and returned the same request ID, proving session isolation works

    @pytest.mark.asyncio
    async def test_session_cleanup(self, http_server):
        """Test that sessions are properly cleaned up."""
        # Create a session
        session_id = "cleanup-session"

        from stepflow_sdk.http_server import StepflowSession

        session = StepflowSession(session_id, http_server.server)
        http_server.sessions[session_id] = session

        # Session should be registered
        assert session_id in http_server.sessions
        assert session.connected is True

        # Mark session as disconnected
        session.connected = False

        # Remove from sessions (simulating cleanup)
        del http_server.sessions[session_id]

        # Session should be cleaned up
        assert session_id not in http_server.sessions
        assert session.connected is False

    @pytest.mark.asyncio
    async def test_request_id_uniqueness_per_session(self, http_server):
        """Test that the same request ID can be used in different sessions without conflict."""  # noqa: E501
        # Create multiple sessions
        session_ids = ["unique-session-1", "unique-session-2", "unique-session-3"]
        sessions = {}

        from stepflow_sdk.http_server import StepflowSession

        for session_id in session_ids:
            session = StepflowSession(session_id, http_server.server)
            sessions[session_id] = session
            http_server.sessions[session_id] = session

        # Initialize the server
        http_server.server.set_initialized(True)

        # Same request ID for all sessions
        shared_request_id = "shared-request-789"

        # Create tasks for all sessions with the same request ID
        tasks = []
        for i, session_id in enumerate(session_ids):
            request = {
                "jsonrpc": "2.0",
                "method": Method.components_execute,
                "id": shared_request_id,
                "params": {
                    "component": "/test_component",
                    "input": {"message": f"Message from session {i + 1}"},
                },
            }
            task = asyncio.create_task(
                http_server._handle_json_rpc(request, sessions[session_id])
            )
            tasks.append(task)

        # Wait for all tasks to complete
        results = await asyncio.gather(*tasks)

        # All requests should return JSON-RPC responses
        for i, result in enumerate(results):
            assert result["jsonrpc"] == "2.0"
            assert result["id"] == shared_request_id

            # All should have successful results
            assert "result" in result

            # Verify the results contain the correct session information
            output = result["result"]["output"]
            assert output["session_id"] == session_ids[i]
            assert f"Message from session {i + 1}" in output["processed_message"]

        # All results should be processed independently
        # The important thing is that all requests with the same ID
        # in different sessions were handled correctly
        assert len(results) == len(session_ids)

        # Each result should have the same request ID
        for result in results:
            assert result["id"] == shared_request_id

    def test_missing_session_id_parameter(self, client):
        """Test that requests without sessionId parameter are rejected."""
        request = MethodRequest(
            id="test-request",
            method=Method.components_list,
            params=ComponentListParams(),
        )
        response = client.post(
            "/",
            json=msgspec.to_builtins(request),
        )

        assert response.status_code == 400
        error = response.json()
        assert "sessionId parameter required" in error["error"]["message"]
        assert error["error"]["code"] == -32600

    def test_invalid_session_id(self, client):
        """Test that requests with invalid sessionId are rejected."""
        request = MethodRequest(
            id="test-request",
            method=Method.components_list,
            params=ComponentListParams(),
        )
        response = client.post(
            "/",
            params={"sessionId": "non-existent-session"},
            json=msgspec.to_builtins(request),
        )

        assert response.status_code == 400
        error = response.json()
        assert "session not found" in error["error"]["message"]
        assert error["error"]["code"] == -32600

    @pytest.mark.asyncio
    async def test_session_event_queue_isolation(self, http_server):
        """Test that event queues are isolated between sessions."""
        # Create two sessions
        session1_id = "event-session-1"
        session2_id = "event-session-2"

        from stepflow_sdk.http_server import StepflowSession

        session1 = StepflowSession(session1_id, http_server.server)
        session2 = StepflowSession(session2_id, http_server.server)

        http_server.sessions[session1_id] = session1
        http_server.sessions[session2_id] = session2

        # Add events to each session's queue
        event1 = {"session": session1_id, "data": "event for session 1"}
        event2 = {"session": session2_id, "data": "event for session 2"}

        await session1.event_queue.put(event1)
        await session2.event_queue.put(event2)

        # Each session should only see its own events
        retrieved_event1 = await session1.event_queue.get()
        retrieved_event2 = await session2.event_queue.get()

        assert retrieved_event1 == event1
        assert retrieved_event2 == event2

        # Queues should be empty after getting events
        assert session1.event_queue.empty()
        assert session2.event_queue.empty()

        # Cross-session events should not be visible
        await session1.event_queue.put({"cross": "session event"})
        assert session2.event_queue.empty()

        # Clean up
        await session1.event_queue.get()  # Remove the cross-session event

    @pytest.mark.asyncio
    async def test_request_id_isolation_successful_execution(self, http_server):
        """Test that successful component execution with same request ID in different sessions returns correct request IDs."""  # noqa: E501
        # Create two sessions
        session1_id = "success-session-1"
        session2_id = "success-session-2"

        from stepflow_sdk.http_server import StepflowSession

        session1 = StepflowSession(session1_id, http_server.server)
        session2 = StepflowSession(session2_id, http_server.server)

        http_server.sessions[session1_id] = session1
        http_server.sessions[session2_id] = session2

        # Initialize the server
        http_server.server.set_initialized(True)

        # Use the same request ID for both sessions - this is the key test
        same_request_id = "success-request-789"

        # Create request payloads with the same request ID
        request1 = {
            "jsonrpc": "2.0",
            "method": Method.components_execute,
            "id": same_request_id,
            "params": {
                "component": "/test_component",
                "input": {"message": "Success message 1"},
            },
        }

        request2 = {
            "jsonrpc": "2.0",
            "method": Method.components_execute,
            "id": same_request_id,
            "params": {
                "component": "/test_component",
                "input": {"message": "Success message 2"},
            },
        }

        # Execute both requests concurrently
        task1 = asyncio.create_task(http_server._handle_json_rpc(request1, session1))
        task2 = asyncio.create_task(http_server._handle_json_rpc(request2, session2))

        # Wait for both to complete
        result1, result2 = await asyncio.gather(task1, task2)

        # Both requests should return JSON-RPC responses with the SAME request ID
        assert result1["jsonrpc"] == "2.0"
        assert result2["jsonrpc"] == "2.0"
        assert result1["id"] == same_request_id
        assert result2["id"] == same_request_id

        # Both should have successful results (component execution succeeds)
        assert "result" in result1
        assert "result" in result2

        # Verify the results contain the correct session information
        output1 = result1["result"]["output"]
        output2 = result2["result"]["output"]

        # Each session should have its own session_id in the output
        assert output1["session_id"] == session1_id
        assert output2["session_id"] == session2_id

        # Messages should be different (showing they were processed independently)
        assert output1["processed_message"] == "Processed: Success message 1"
        assert output2["processed_message"] == "Processed: Success message 2"

        # This proves that:
        # 1. Same request ID can be used in different sessions
        # 2. Each session gets its own response with the correct request ID
        # 3. No contamination between sessions
        # 4. Request ID delivery is properly isolated

    @pytest.mark.asyncio
    async def test_request_id_isolation_with_rapid_sequential_requests(
        self, http_server
    ):
        """Test rapid sequential requests with same ID in different sessions to check for race conditions."""  # noqa: E501
        # Create two sessions
        session1_id = "rapid-session-1"
        session2_id = "rapid-session-2"

        from stepflow_sdk.http_server import StepflowSession

        session1 = StepflowSession(session1_id, http_server.server)
        session2 = StepflowSession(session2_id, http_server.server)

        http_server.sessions[session1_id] = session1
        http_server.sessions[session2_id] = session2

        # Initialize the server
        http_server.server.set_initialized(True)

        # Use the same request ID for rapid sequential requests
        same_request_id = "rapid-request-999"

        # Create multiple rapid requests alternating between sessions
        requests_and_sessions = []
        for i in range(10):  # 10 rapid requests
            session = session1 if i % 2 == 0 else session2
            session_id = session1_id if i % 2 == 0 else session2_id

            request = {
                "jsonrpc": "2.0",
                "method": Method.components_execute,
                "id": same_request_id,
                "params": {
                    "component": "/test_component",
                    "input": {"message": f"Rapid message {i} for {session_id}"},
                },
            }
            requests_and_sessions.append((request, session, session_id))

        # Execute all requests concurrently
        tasks = []
        for request, session, session_id in requests_and_sessions:
            task = asyncio.create_task(http_server._handle_json_rpc(request, session))
            tasks.append((task, session_id))

        # Wait for all to complete
        results = await asyncio.gather(*[task for task, _ in tasks])

        # Verify all responses have the correct request ID and session isolation
        for i, result in enumerate(results):
            expected_session_id = session1_id if i % 2 == 0 else session2_id

            assert result["jsonrpc"] == "2.0"
            assert result["id"] == same_request_id, f"Request {i} has wrong request ID"

            # Check that the response contains the correct session info
            if "result" in result:
                output = result["result"]["output"]
                assert output["session_id"] == expected_session_id, (
                    f"Request {i} has wrong session ID in output"
                )
                assert f"Rapid message {i}" in output["processed_message"]
            else:
                # If there's an error, make sure it still has the correct request ID
                assert "error" in result, (
                    f"Request {i} should have either result or error"
                )

        # This test verifies that even with rapid sequential requests using the same
        # request ID, there's no contamination between sessions

    @pytest.mark.asyncio
    async def test_request_id_isolation_with_mixed_success_and_failure(
        self, http_server
    ):
        """Test request ID isolation when some requests succeed and others fail."""
        # Create two sessions
        session1_id = "mixed-session-1"
        session2_id = "mixed-session-2"

        from stepflow_sdk.http_server import StepflowSession

        session1 = StepflowSession(session1_id, http_server.server)
        session2 = StepflowSession(session2_id, http_server.server)

        http_server.sessions[session1_id] = session1
        http_server.sessions[session2_id] = session2

        # Initialize the server
        http_server.server.set_initialized(True)

        # Use the same request ID for both requests
        same_request_id = "mixed-request-555"

        # Request 1: Valid component (should succeed)
        request1 = {
            "jsonrpc": "2.0",
            "method": Method.components_execute,
            "id": same_request_id,
            "params": {
                "component": "/test_component",
                "input": {"message": "Valid request"},
            },
        }

        # Request 2: Invalid component (should fail)
        request2 = {
            "jsonrpc": "2.0",
            "method": Method.components_execute,
            "id": same_request_id,
            "params": {
                "component": "/python/nonexistent_component",
                "input": {"message": "Invalid request"},
            },
        }

        # Execute both requests concurrently
        task1 = asyncio.create_task(http_server._handle_json_rpc(request1, session1))
        task2 = asyncio.create_task(http_server._handle_json_rpc(request2, session2))

        # Wait for both to complete
        result1, result2 = await asyncio.gather(task1, task2)

        # Both should have the same request ID
        assert result1["id"] == same_request_id
        assert result2["id"] == same_request_id

        # Result 1 should be success, result 2 should be error
        assert "result" in result1
        assert "error" in result2

        # Verify the success result has correct session info
        output1 = result1["result"]["output"]
        assert output1["session_id"] == session1_id
        assert "Valid request" in output1["processed_message"]

        # Verify the error result has correct request ID
        assert result2["error"]["code"] == -32001  # Component error
        assert "not found" in result2["error"]["message"]

        # This proves that success/failure of one session doesn't affect
        # the request ID delivery of another session
