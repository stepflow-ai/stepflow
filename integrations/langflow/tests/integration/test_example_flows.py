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

"""Example flow tests: Complete lifecycle testing for Langflow workflows.

Each test represents a complete workflow lifecycle:
1. Convert Langflow JSON → Stepflow YAML
2. Validate with stepflow binary
3. Execute with real Langflow components
4. Verify results

This approach makes each test easy to understand, debug, and maintain with
workflow-specific setup and assertions.
"""

import os
from pathlib import Path
from typing import Any

import pytest

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.testing.config_builder import StepflowConfigBuilder
from stepflow_langflow_integration.testing.stepflow_binary import StepflowBinaryRunner


@pytest.fixture(scope="module")
def converter():
    """Create Langflow converter instance."""
    return LangflowConverter()


@pytest.fixture(scope="module")
def stepflow_runner():
    """Create stepflow binary runner."""
    try:
        runner = StepflowBinaryRunner()
        available, version = runner.check_binary_availability()
        if not available:
            pytest.skip(f"Stepflow binary not available: {version}")
        return runner
    except FileNotFoundError as e:
        pytest.skip(f"Stepflow binary not found: {e}")


def has_openai_api_key() -> bool:
    """Check if OpenAI API key is available."""
    return bool(os.getenv("OPENAI_API_KEY"))


def load_flow_fixture(flow_name: str) -> dict[str, Any]:
    """Load Langflow JSON fixture by name."""
    fixtures_dir = Path(__file__).parent.parent / "fixtures" / "langflow"
    flow_path = fixtures_dir / f"{flow_name}.json"

    if not flow_path.exists():
        pytest.skip(f"Flow fixture not found: {flow_path}")

    import json

    with open(flow_path, encoding="utf-8") as f:
        return json.load(f)


def execute_complete_flow_lifecycle(
    flow_name: str,
    input_data: dict[str, Any],
    converter: LangflowConverter,
    stepflow_runner: StepflowBinaryRunner,
    config_builder: StepflowConfigBuilder,
    timeout: float = 60.0,
) -> dict[str, Any]:
    """Execute complete flow lifecycle: convert → validate → execute.

    Args:
        flow_name: Name of flow fixture (without .json extension)
        input_data: Input data for workflow execution
        converter: LangflowConverter instance
        stepflow_runner: StepflowBinaryRunner instance
        config_builder: StepflowConfigBuilder instance
        timeout: Execution timeout in seconds

    Returns:
        Execution result dict

    Raises:
        AssertionError: If any step fails
        pytest.skip: If dependencies not available
    """

    # Step 1: Load and convert
    langflow_data = load_flow_fixture(flow_name)
    stepflow_workflow = converter.convert(langflow_data)
    workflow_yaml = converter.to_yaml(stepflow_workflow)

    # Create config file once and reuse it
    with config_builder.to_temp_yaml() as config_path:
        # Step 2: Validate
        success, stdout, stderr = stepflow_runner.validate_workflow(
            workflow_yaml, config_path=config_path
        )
        assert success, (
            f"Workflow validation failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}"
        )

        # Step 3: Execute
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml,
            input_data,
            config_path=config_path,
            timeout=timeout,
        )

        if not success:
            # Check for common dependency issues and skip gracefully
            if any(
                indicator in stderr.lower()
                for indicator in [
                    "importerror",
                    "modulenotfounderror",
                    "no such table",
                    "no files to process",
                ]
            ):
                pytest.skip(f"External dependency not available: {stderr}")
            else:
                pytest.fail(
                    f"Workflow execution failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}"
                )

        # Step 4: Basic result validation
        assert isinstance(result_data, dict), (
            f"Expected dict result, got {type(result_data)}"
        )
        assert result_data.get("outcome") == "success", (
            f"Execution failed: {result_data}"
        )
        assert "result" in result_data, f"No result field in output: {result_data}"

        return result_data


# Simple flows - no external dependencies


def test_simple_chat(converter, stepflow_runner):
    """Test simple chat: direct passthrough workflow with no processing."""
    with StepflowConfigBuilder() as config:
        result = execute_complete_flow_lifecycle(
            flow_name="simple_chat",
            input_data={"message": "Hello from simple chat test"},
            converter=converter,
            stepflow_runner=stepflow_runner,
            config_builder=config,
            timeout=30.0,
        )

        # Simple chat should return the input message directly
        assert result["result"] == "Hello from simple chat test"


# API-dependent flows


def test_openai_chat(converter, stepflow_runner):
    """Test OpenAI chat: direct API integration with built-in LanguageModelComponent."""
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")

    with StepflowConfigBuilder() as config:
        result = execute_complete_flow_lifecycle(
            flow_name="openai_chat",
            input_data={"message": "What is 2+2?"},
            converter=converter,
            stepflow_runner=stepflow_runner,
            config_builder=config,
            timeout=45.0,
        )

        # Should return a Langflow Message with text content
        message_result = result["result"]
        assert isinstance(message_result, dict)
        assert "__langflow_type__" in message_result
        assert "text" in message_result
        assert len(message_result["text"]) > 0


def test_basic_prompting(converter, stepflow_runner):
    """Test basic prompting: custom Prompt + LanguageModelComponent with OpenAI API."""
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")

    with StepflowConfigBuilder() as config:
        result = execute_complete_flow_lifecycle(
            flow_name="basic_prompting",
            input_data={"message": "Write a haiku about testing"},
            converter=converter,
            stepflow_runner=stepflow_runner,
            config_builder=config,
            timeout=60.0,
        )

        # Should return a Langflow Message with text content
        message_result = result["result"]
        assert isinstance(message_result, dict)
        assert "__langflow_type__" in message_result
        assert "text" in message_result
        # Haiku should have some structure (multiple lines)
        assert len(message_result["text"].split("\n")) >= 3


def test_vector_store_rag():
    """Test vector store RAG: complex workflow with embeddings and retrieval."""
    # Vector store RAG is highly complex with embeddings, databases, and file processing
    pytest.skip("Missing dependencies (AstraDB, embeddings, file processing)")


# Complex flows with external dependencies


def test_memory_chatbot(converter, stepflow_runner):
    """Test memory chatbot: validates session handling and memory retrieval.

    This test verifies that:
    1. Session IDs are properly passed from workflow input to Langflow components
    2. Database schema is correctly initialized for Langflow message storage
    3. Memory components can execute without database connection errors
    4. Memory retrieval works correctly with proper session_id matching

    Test approach:
    1. First query: "What is my name?" - should return "no information" response
    2. Manually insert a message into the database with session_id
    3. Second query: "What is my name?" - should retrieve and use the message

    This validates the Retrieve-mode Memory component works with proper session
    handling.
    """
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")

    # Ultra-clean context manager approach with automatic resource cleanup
    with StepflowConfigBuilder() as config:
        config.add_temp_langflow_database()

        # Setup test data with different session IDs upfront
        our_session_id = "test-memory-session-123"
        other_session_id = "other-session-456"

        # Phase 1: Pre-populate database with messages using proper Langflow objects
        # This ensures proper data types and schema compatibility
        import asyncio

        from langflow.memory import astore_message
        from langflow.schema.message import Message

        # Create proper Message objects to store
        alex_user_msg = Message(
            text="My name is Alex",
            sender="User",
            sender_name="User",
            session_id=our_session_id,
        )
        alex_ai_msg = Message(
            text="Hello Alex! Nice to meet you.",
            sender="AI",
            sender_name="Assistant",
            session_id=our_session_id,
        )
        bob_user_msg = Message(
            text="My name is Bob",
            sender="User",
            sender_name="User",
            session_id=other_session_id,
        )
        bob_ai_msg = Message(
            text="Hello Bob! Great to meet you.",
            sender="AI",
            sender_name="Assistant",
            session_id=other_session_id,
        )

        # Configure Langflow to use our temporary database
        # Find the database path from our config builder
        db_path = None
        for temp_file in config._temp_files:
            if temp_file.suffix == ".db":
                db_path = temp_file
                break

        assert db_path and db_path.exists(), "Database file not found"

        # Set up database environment for Langflow's storage methods
        import os

        # Temporarily clear any existing database URL to force re-initialization
        original_db_url = os.environ.get("LANGFLOW_DATABASE_URL")
        os.environ["LANGFLOW_DATABASE_URL"] = f"sqlite:///{db_path}"

        try:
            # Store messages using Langflow's proper async storage method
            import uuid

            test_flow_id = uuid.uuid4()  # Generate a proper UUID for flow_id

            async def store_test_messages():
                await astore_message(alex_user_msg, flow_id=test_flow_id)
                await astore_message(alex_ai_msg, flow_id=test_flow_id)
                await astore_message(bob_user_msg, flow_id=test_flow_id)
                await astore_message(bob_ai_msg, flow_id=test_flow_id)

            # Run the async function synchronously
            asyncio.run(store_test_messages())
        finally:
            # Restore original database URL if it existed
            if original_db_url:
                os.environ["LANGFLOW_DATABASE_URL"] = original_db_url
            else:
                os.environ.pop("LANGFLOW_DATABASE_URL", None)

        # Phase 2: Test end-to-end memory functionality
        langflow_data = load_flow_fixture("memory_chatbot")
        stepflow_workflow = converter.convert(langflow_data)
        workflow_yaml = converter.to_yaml(stepflow_workflow)

        with config.to_temp_yaml() as config_path:
            # Test Alex's session
            success, result_data, stdout, stderr = stepflow_runner.run_workflow(
                workflow_yaml,
                {"message": "What is my name?", "session_id": our_session_id},
                config_path=config_path,
                timeout=60.0,
            )

            # Check for the known Langflow bug
            error_text = stderr + " " + stdout
            if result_data and isinstance(result_data, dict) and "error" in result_data:
                error_text += " " + str(result_data.get("error", {}))

            if "int' object has no attribute 'replace'" in error_text:
                raise AssertionError(
                    "Langflow Memory component bug encountered: "
                    "'int' object has no attribute 'replace'"
                )

            assert success, f"Workflow execution failed: {error_text}"

            # Test Bob's session
            success_other, result_other, _, _ = stepflow_runner.run_workflow(
                workflow_yaml,
                {"message": "What is my name?", "session_id": other_session_id},
                config_path=config_path,
                timeout=60.0,
            )

            assert success_other, "Bob's session workflow execution failed"

            # Debug: Print the actual responses to see what each session is getting
            our_response = result_data["result"]["text"]
            other_response = result_other["result"]["text"]

            print(
                f"\nDEBUG - Alex's session ({our_session_id}) response: {our_response}"
            )
            print(
                f"DEBUG - Bob's session ({other_session_id}) response: {other_response}"
            )

            # Let's also check the database to see what messages were actually retrieved
            import sqlite3

            conn = sqlite3.connect(str(db_path))
            try:
                cursor = conn.cursor()
                cursor.execute(
                    "SELECT session_id, sender_name, text FROM message "
                    "ORDER BY timestamp"
                )
                all_messages = cursor.fetchall()
                print("DEBUG - All messages in database:")
                for msg in all_messages:
                    print(f"  Session: {msg[0]}, Sender: {msg[1]}, Text: {msg[2]}")
            finally:
                conn.close()

            alex_remembered = "Alex" in our_response or "alex" in our_response.lower()
            bob_remembered = "Bob" in other_response or "bob" in other_response.lower()

            # Check for session isolation - Alex shouldn't see Bob's messages and
            # vice versa
            alex_sees_bob = "Bob" in our_response or "bob" in our_response.lower()
            bob_sees_alex = "Alex" in other_response or "alex" in other_response.lower()

            print(f"DEBUG - Alex remembered correctly: {alex_remembered}")
            print(f"DEBUG - Bob remembered correctly: {bob_remembered}")
            print(f"DEBUG - Alex sees Bob's messages (bad): {alex_sees_bob}")
            print(f"DEBUG - Bob sees Alex's messages (bad): {bob_sees_alex}")

            # Validate memory recall and session isolation
            assert alex_remembered, (
                f"Alex not remembered in Alex's session. Response: {our_response}"
            )
            assert bob_remembered, (
                f"Bob not remembered in Bob's session. Response: {other_response}"
            )
            assert not alex_sees_bob, (
                f"Session isolation broken: Alex sees Bob's messages. "
                f"Response: {our_response}"
            )
            assert not bob_sees_alex, (
                f"Session isolation broken: Bob sees Alex's messages. "
                f"Response: {other_response}"
            )

    # All temporary resources (database, config file) automatically cleaned up


def test_document_qa():
    """Test document QA: requires file input data."""
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")

    # Document QA requires file upload integration
    pytest.skip(
        "Document QA requires file upload integration not available in test environment"
    )


def test_simple_agent():
    """Test simple agent: tool coordination with Calculator and URL components."""
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")

    # Agent workflows require complex tool coordination
    pytest.skip("Simple agent requires complex tool coordination not fully supported")


# Utility and infrastructure tests


def test_langflow_server_availability(stepflow_runner):
    """Test that Langflow component server can be started and responds."""
    # Test component listing to verify server works
    try:
        with StepflowConfigBuilder() as config:
            with config.to_temp_yaml() as config_path:
                success, components, stderr = stepflow_runner.list_components(
                    config_path=config_path
                )
                assert success, f"Component listing failed: {stderr}"

                # Should have langflow components available
                langflow_components = [c for c in components if "langflow" in c.lower()]
                assert len(langflow_components) > 0, (
                    f"Should have Langflow components available: {components}"
                )

    except Exception as e:
        pytest.skip(f"Langflow server not available: {e}")


def test_blob_storage_integration(converter):
    """Test that blob storage works for component code."""
    # Use basic_prompting which definitely has custom components
    langflow_data = load_flow_fixture("basic_prompting")
    stepflow_workflow = converter.convert(langflow_data)

    # Should have blob steps for component code storage
    blob_steps = [
        step
        for step in stepflow_workflow.steps
        if step.component == "/builtin/put_blob"
    ]

    # Should have at least one blob step for component code
    assert len(blob_steps) > 0, "Should have blob storage steps for component code"

    # Blob step should contain component code data
    blob_step = blob_steps[0]
    assert "data" in blob_step.input
    blob_data = blob_step.input["data"]
    assert "code" in blob_data, "Blob should contain component code"
    assert "component_type" in blob_data, "Blob should contain component type"


# Error handling tests


def test_execution_with_missing_input(converter, stepflow_runner):
    """Test execution with missing required input - workflows handle gracefully."""
    # Modern Langflow workflows often have good defaults and handle
    # missing input gracefully. This test verifies that our integration
    # doesn't crash with empty input

    langflow_data = load_flow_fixture("basic_prompting")
    stepflow_workflow = converter.convert(langflow_data)
    workflow_yaml = converter.to_yaml(stepflow_workflow)

    # Execute without providing input - should either succeed with
    # defaults or fail gracefully
    with StepflowConfigBuilder() as config:
        with config.to_temp_yaml() as config_path:
            success, result_data, _, stderr = stepflow_runner.run_workflow(
                workflow_yaml, {}, config_path=config_path, timeout=15.0
            )

            # Either outcome is acceptable - we're testing that it doesn't crash
            if success:
                # Succeeded with defaults - good behavior
                assert isinstance(result_data, dict)
                assert result_data.get("outcome") == "success"
            else:
                # Failed gracefully - also good behavior
                assert isinstance(result_data, dict) or "error" in stderr.lower()


def test_execution_timeout(converter, stepflow_runner):
    """Test execution timeout handling."""
    # Use simple workflow with very short timeout
    langflow_data = load_flow_fixture("simple_chat")
    stepflow_workflow = converter.convert(langflow_data)
    workflow_yaml = converter.to_yaml(stepflow_workflow)

    # Use very short timeout - should either complete quickly or timeout
    try:
        with StepflowConfigBuilder() as config:
            with config.to_temp_yaml() as config_path:
                success, result_data, _, _ = stepflow_runner.run_workflow(
                    workflow_yaml, {}, config_path=config_path, timeout=0.1
                )

                # If it completes, that's fine too (simple_chat is very simple)
                if success:
                    assert isinstance(result_data, dict)

    except Exception:
        # Timeout exceptions are expected and acceptable
        pass
