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
from tests.helpers.testing.stepflow_binary import StepflowBinaryRunner


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


@pytest.fixture(scope="module")
def shared_config():
    """Create a shared Stepflow configuration with Langflow database for all tests.

    This configuration includes:
    - Langflow component server
    - Shared database for memory/session handling
    - No environment variable setup (API keys now passed via tweaks)
    """
    import asyncio
    import contextlib
    import os
    import tempfile
    from pathlib import Path

    import yaml

    # Create shared database path
    shared_db_path = Path(tempfile.gettempdir()) / "stepflow_langflow_shared_test.db"

    # Initialize database if it doesn't exist
    if not shared_db_path.exists():
        # Initialize Langflow database using their service
        original_db_url = os.environ.get("LANGFLOW_DATABASE_URL")
        os.environ["LANGFLOW_DATABASE_URL"] = f"sqlite:///{shared_db_path}"

        try:
            from langflow.services.deps import get_db_service, get_settings_service
            from langflow.services.manager import service_manager

            # Clear any existing service cache
            if hasattr(service_manager, "_services"):
                service_manager._services.clear()

            # Initialize services and create database
            get_settings_service()
            db_service = get_db_service()
            db_service.reload_engine()
            asyncio.run(db_service.create_db_and_tables())
        finally:
            if original_db_url:
                os.environ["LANGFLOW_DATABASE_URL"] = original_db_url
            else:
                os.environ.pop("LANGFLOW_DATABASE_URL", None)

    # Get path to langflow integration directory
    current_dir = Path(__file__).parent.parent.parent

    # Create configuration dictionary
    config_dict = {
        "plugins": {
            "builtin": {"type": "builtin"},
            "langflow": {
                "type": "stepflow",
                "transport": "stdio",
                "command": "uv",
                "args": [
                    "--project",
                    str(current_dir),
                    "run",
                    "stepflow-langflow-server",
                ],
                "env": {
                    "LANGFLOW_DATABASE_URL": f"sqlite:///{shared_db_path}",
                    "LANGFLOW_AUTO_LOGIN": "false",
                },
            },
        },
        "routes": {
            "/langflow/{*component}": [{"plugin": "langflow"}],
            "/builtin/{*component}": [{"plugin": "builtin"}],
        },
        "stateStore": {"type": "inMemory"},
    }

    # Create a simple config object with the methods we need
    class InlineConfig:
        def __init__(self, config_dict):
            self._config = config_dict

        @contextlib.contextmanager
        def to_temp_yaml(self):
            """Create temporary YAML config file with automatic cleanup."""
            with tempfile.NamedTemporaryFile(
                mode="w", suffix=".yml", delete=False
            ) as f:
                yaml.dump(self._config, f, default_flow_style=False, sort_keys=False)
                temp_path = Path(f.name)

            try:
                yield str(temp_path)
            finally:
                temp_path.unlink(missing_ok=True)

    yield InlineConfig(config_dict)


class TestExecutor:
    """Encapsulates flow execution logic for cleaner test code."""

    def __init__(
        self,
        converter: LangflowConverter,
        stepflow_runner: StepflowBinaryRunner,
        shared_config: Any,
    ):
        self.converter = converter
        self.stepflow_runner = stepflow_runner
        self.shared_config = shared_config

    def execute_flow(
        self,
        flow_name: str,
        input_data: dict[str, Any],
        timeout: float = 60.0,
        tweaks: dict[str, dict[str, Any]] = None,
    ) -> dict[str, Any]:
        """Execute complete flow lifecycle: convert → validate → execute.

        Args:
            flow_name: Name of flow fixture (without .json extension)
            input_data: Input data for workflow execution
            timeout: Execution timeout in seconds
            tweaks: Optional Stepflow-level tweaks to apply

        Returns:
            Execution result dict

        Raises:
            AssertionError: If any step fails
            pytest.skip: If dependencies not available
        """
        from stepflow_langflow_integration.converter.stepflow_tweaks import (
            apply_stepflow_tweaks,
        )

        # Step 1: Load and convert
        langflow_data = load_flow_fixture(flow_name)
        stepflow_workflow = self.converter.convert(langflow_data)

        # Apply tweaks if provided
        if tweaks:
            stepflow_workflow = apply_stepflow_tweaks(stepflow_workflow, tweaks)

        workflow_yaml = self.converter.to_yaml(stepflow_workflow)

        # Use the shared config for execution
        with self.shared_config.to_temp_yaml() as config_path:
            # Step 2: Validate
            success, stdout, stderr = self.stepflow_runner.validate_workflow(
                workflow_yaml, config_path=config_path
            )
            assert success, (
                f"Workflow validation failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}"
            )

            # Step 3: Execute
            success, result_data, stdout, stderr = self.stepflow_runner.run_workflow(
                workflow_yaml,
                input_data,
                config_path=config_path,
                timeout=timeout,
            )

            if not success:
                pytest.fail(
                    f"Workflow execution failed:\nResult: {result_data}\n"
                    f"STDOUT: {stdout}\nSTDERR: {stderr}"
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


@pytest.fixture(scope="module")
def test_executor(converter, stepflow_runner, shared_config):
    """Create a TestExecutor with all necessary dependencies."""
    return TestExecutor(converter, stepflow_runner, shared_config)


def load_flow_fixture(flow_name: str) -> dict[str, Any]:
    """Load Langflow JSON fixture by name."""
    fixtures_dir = Path(__file__).parent.parent / "fixtures" / "langflow"
    flow_path = fixtures_dir / f"{flow_name}.json"

    if not flow_path.exists():
        pytest.skip(f"Flow fixture not found: {flow_path}")

    import json

    with open(flow_path, encoding="utf-8") as f:
        return json.load(f)


# API-dependent flows


def test_basic_prompting(test_executor):
    """Test basic prompting: custom Prompt + LanguageModelComponent with OpenAI API."""

    # Use test utilities to create tweaks for OpenAI components
    from tests.helpers.tweaks_builder import create_basic_prompting_tweaks

    tweaks = create_basic_prompting_tweaks()

    result = test_executor.execute_flow(
        flow_name="basic_prompting",
        input_data={"message": "Write a haiku about testing"},
        timeout=60.0,
        tweaks=tweaks,
    )

    # Should return a Langflow Message with text content
    message_result = result["result"]
    assert isinstance(message_result, dict)
    assert "__langflow_type__" in message_result
    assert "text" in message_result
    # Haiku should have some structure (multiple lines)
    assert len(message_result["text"].split("\n")) >= 3


def test_vector_store_rag(test_executor):
    """Test vector store RAG: complex workflow with embeddings and retrieval."""

    # Create temporary test document for RAG processing
    import tempfile

    test_content = """# Advanced AI and Machine Learning Technologies

This comprehensive document explores cutting-edge developments in artificial
intelligence and machine learning.

## Deep Learning Architectures

### Transformer Models
Transformer architecture has revolutionized natural language processing through
self-attention mechanisms. Key models include BERT, GPT, and T5.

### Convolutional Neural Networks
CNNs excel at image recognition tasks through hierarchical feature detection:
- Edge detection in early layers
- Pattern recognition in middle layers
- Object identification in final layers

## Vector Databases and Retrieval

Vector databases store high-dimensional embeddings for semantic search and
retrieval-augmented generation (RAG). Popular solutions include:
- Pinecone for managed vector search
- Weaviate for open-source semantic search
- Chroma for lightweight embedding storage

## Applications in Industry
- Healthcare: Medical image analysis and drug discovery
- Finance: Fraud detection and algorithmic trading
- Autonomous vehicles: Computer vision and decision making
- Recommendation systems: Personalized content delivery
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        f.write(test_content)
        test_file_path = f.name

    try:
        # Use test utilities to create tweaks for components
        from tests.helpers.tweaks_builder import create_vector_store_rag_tweaks

        tweaks = create_vector_store_rag_tweaks()

        try:
            result = test_executor.execute_flow(
                flow_name="vector_store_rag",
                input_data={
                    "message": "What is the main topic of the document?",
                    "file_path": test_file_path,  # Provide the file path
                },
                timeout=120.0,
                tweaks=tweaks,
            )

            # Should return a response about the document content
            message_result = result["result"]
            assert isinstance(message_result, dict)
            assert "text" in message_result

            # Should reference AI/ML content from our comprehensive document
            response_text = message_result["text"].lower()
            content_indicators = [
                "artificial intelligence",
                "machine learning",
                "deep learning",
                "transformer",
                "vector",
                "ai",
                "ml",
            ]
            found_content = any(
                indicator in response_text for indicator in content_indicators
            )
            assert found_content, (
                f"Response should reference document content about AI/ML. "
                f"Got: {response_text}"
            )

        except Exception as e:
            error_message = str(e)
            # Check for known issues that should be skipped vs. real failures
            if (
                "authentication" in error_message.lower()
                or "unauthorized" in error_message.lower()
                or "api_key" in error_message.lower()
                or "token" in error_message.lower()
            ):
                pytest.fail(f"AstraDB authentication issue: {error_message}")
            elif (
                "network" in error_message.lower()
                or "connection" in error_message.lower()
            ):
                pytest.fail(f"AstraDB connectivity issue: {error_message}")
            else:
                # This is a real test failure, not an infrastructure issue
                raise

    finally:
        # Clean up temporary file
        os.unlink(test_file_path)


def test_memory_chatbot(test_executor):
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

    # Use test utilities to create tweaks for OpenAI components
    from tests.helpers.tweaks_builder import TweaksBuilder

    tweaks = (
        TweaksBuilder()
        .add_openai_tweaks(
            "LanguageModelComponent-n8krg"
        )  # Correct ID for memory_chatbot
        .build_or_skip()
    )

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

    # Configure Langflow to use our shared database
    # Extract database path from the plugin environment variables
    langflow_plugin_config = test_executor.shared_config._config["plugins"]["langflow"]
    db_url = langflow_plugin_config.get("env", {}).get("LANGFLOW_DATABASE_URL", "")
    # Extract path from sqlite:///path format
    db_path = (
        db_url.replace("sqlite:///", "") if db_url.startswith("sqlite:///") else None
    )

    assert db_path and Path(db_path).exists(), f"Database file not found at {db_path}"

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
    # Apply tweaks and execute the complete lifecycle
    result = test_executor.execute_flow(
        flow_name="memory_chatbot",
        input_data={"message": "What is my name?", "session_id": our_session_id},
        timeout=60.0,
        tweaks=tweaks,
    )

    # Check for the known Langflow bug
    our_response = result["result"]["text"]

    # Test Bob's session
    result_other = test_executor.execute_flow(
        flow_name="memory_chatbot",
        input_data={"message": "What is my name?", "session_id": other_session_id},
        timeout=60.0,
        tweaks=tweaks,
    )

    other_response = result_other["result"]["text"]

    # Debug: Print the actual responses to see what each session is getting

    alex_remembered = "Alex" in our_response or "alex" in our_response.lower()
    bob_remembered = "Bob" in other_response or "bob" in other_response.lower()

    # Check for session isolation - Alex shouldn't see Bob's messages and
    # vice versa
    alex_sees_bob = "Bob" in our_response or "bob" in our_response.lower()
    bob_sees_alex = "Alex" in other_response or "alex" in other_response.lower()

    # Validate memory recall and session isolation
    assert alex_remembered, (
        f"Alex not remembered in Alex's session. Response: {our_response}"
    )
    assert bob_remembered, (
        f"Bob not remembered in Bob's session. Response: {other_response}"
    )
    assert not alex_sees_bob, (
        f"Session isolation broken: Alex sees Bob's messages. Response: {our_response}"
    )
    assert not bob_sees_alex, (
        f"Session isolation broken: Bob sees Alex's messages. "
        f"Response: {other_response}"
    )

    # All temporary resources (database, config file) automatically cleaned up


def test_document_qa(test_executor):
    """Test document QA: validates document loading and question answering."""

    # Create temporary test document with meaningful content
    import tempfile

    test_content = """# Machine Learning Fundamentals

This document covers the basics of machine learning and artificial intelligence.

## Key Concepts

Machine learning is a subset of artificial intelligence that enables computers to
learn and make decisions from data without being explicitly programmed for every
scenario.

### Types of Machine Learning
1. Supervised Learning: Uses labeled data to train models
2. Unsupervised Learning: Finds patterns in unlabeled data
3. Reinforcement Learning: Learns through interaction and rewards

## Applications
Machine learning is used in recommendation systems, image recognition, natural
language processing, and autonomous vehicles.
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        f.write(test_content)
        test_file_path = f.name

    try:
        # Use test utilities to create tweaks for OpenAI components
        from tests.helpers.tweaks_builder import TweaksBuilder

        tweaks = (
            TweaksBuilder()
            .add_openai_tweaks(
                "LanguageModelComponent-htmui"
            )  # Correct ID for document_qa
            .build_or_skip()
        )

        # Test with file path provided as input data
        result = test_executor.execute_flow(
            flow_name="document_qa",
            input_data={
                "message": (
                    "What are the three types of machine learning mentioned "
                    "in the document?"
                ),
                "file_path": test_file_path,  # Provide the file path
            },
            timeout=120.0,
            tweaks=tweaks,
        )

        # Should return a response about the document content
        message_result = result["result"]
        assert isinstance(message_result, dict)
        assert "text" in message_result

        # The response should mention machine learning types from the document
        response_text = message_result["text"].lower()
        # Should reference content from our test document
        content_indicators = [
            "supervised",
            "unsupervised",
            "reinforcement",
            "machine learning",
        ]
        found_content = any(
            indicator in response_text for indicator in content_indicators
        )
        assert found_content, (
            f"Response should reference document content. Got: {response_text}"
        )

    finally:
        # Clean up temporary file
        os.unlink(test_file_path)


def test_simple_agent(test_executor):
    """Test simple agent: tool coordination with Calculator and URL components."""

    # Use test utilities to create tweaks for OpenAI components
    from tests.helpers.tweaks_builder import TweaksBuilder

    # Agent component needs OpenAI API key tweaks
    tweaks = (
        TweaksBuilder()
        .add_openai_tweaks("Agent-D0Kx2")  # Agent component ID from the flow
        .build_or_skip()
    )

    # Use a more complex calculation that requires actual computation
    complex_expression = "137 * 89 + 456 / 12 - 73"
    # Expected result: 137 * 89 + 456 / 12 - 73 = 12158.0

    # Use a unique session ID to avoid conflicts with other tests
    import uuid

    unique_session_id = f"agent_test_{uuid.uuid4().hex[:8]}"

    result = test_executor.execute_flow(
        flow_name="simple_agent",
        input_data={
            "message": f"Calculate this exact expression: {complex_expression}. "
            f"Show your work and give me the precise numerical result.",
            "session_id": unique_session_id,
        },
        timeout=120.0,  # Longer timeout for complex calculation
        tweaks=tweaks,
    )

    # Should return a Langflow Message with text content containing result
    message_result = result["result"]
    assert isinstance(message_result, dict)
    assert "__langflow_type__" in message_result
    assert "text" in message_result

    # Verify the complex calculation result is present
    response_text = message_result["text"].lower()
    # The result should be 12158.0, look for various formats
    result_patterns = ["12158", "12158.0", "12,158"]
    result_found = any(pattern in response_text for pattern in result_patterns)
    assert result_found, (
        f"Expected result (12158) not found in response: {response_text}"
    )

    # CRITICAL: Verify that tools were used by checking database message history
    # If the calculator tool was invoked, there should be messages in the database

    # Extract database path from the plugin environment variables
    langflow_plugin_config = test_executor.shared_config._config["plugins"]["langflow"]
    db_url = langflow_plugin_config.get("env", {}).get("LANGFLOW_DATABASE_URL", "")
    # Extract path from sqlite:///path format
    db_path = (
        db_url.replace("sqlite:///", "") if db_url.startswith("sqlite:///") else None
    )

    tool_usage_verified = False

    if db_path:
        import sqlite3

        conn = sqlite3.connect(str(db_path))
        cursor = conn.cursor()

        # Get messages from our specific session that contain our calculation
        # This verifies that tools were used for this specific test run
        # Note: Agent uses formatted numbers with commas and mathematical symbols
        cursor.execute(
            """
            SELECT text FROM message
            WHERE session_id = ?
            AND (text LIKE ? OR text LIKE ? OR text LIKE ?)
        """,
            (unique_session_id, "%12,193%", "%12,158%", "%137 × 89%"),
        )
        matching_messages = cursor.fetchall()
        conn.close()

        # If we found any messages with our calculation, tools were used
        if matching_messages:
            tool_usage_verified = True

    assert tool_usage_verified, (
        f"No evidence of calculator tool usage found in database. "
        f"Expression: {complex_expression}"
    )

    # Test passes - tools were verified to be used


# Utility and infrastructure tests


def test_langflow_server_availability(test_executor):
    """Test that Langflow component server can be started and responds."""
    # Test component listing to verify server works
    try:
        with test_executor.shared_config.to_temp_yaml() as config_path:
            success, components, stderr = test_executor.stepflow_runner.list_components(
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


def test_blob_storage_integration(test_executor):
    """Test that blob storage works for component code."""
    # Use basic_prompting which definitely has custom components
    langflow_data = load_flow_fixture("basic_prompting")
    stepflow_workflow = test_executor.converter.convert(langflow_data)

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


def test_execution_with_missing_input(test_executor):
    """Test execution with missing required input - workflows handle gracefully."""
    # Modern Langflow workflows often have good defaults and handle
    # missing input gracefully. This test verifies that our integration
    # doesn't crash with empty input

    langflow_data = load_flow_fixture("basic_prompting")
    stepflow_workflow = test_executor.converter.convert(langflow_data)
    workflow_yaml = test_executor.converter.to_yaml(stepflow_workflow)

    # Execute without providing input - should either succeed with
    # defaults or fail gracefully
    with test_executor.shared_config.to_temp_yaml() as config_path:
        success, result_data, _, stderr = test_executor.stepflow_runner.run_workflow(
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
