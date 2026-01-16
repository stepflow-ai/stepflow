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
1. Convert Langflow JSON → Stepflow workflow
2. Store and validate via API (store_flow returns diagnostics)
3. Execute with real Langflow components
4. Verify results

This approach makes each test easy to understand, debug, and maintain with
workflow-specific setup and assertions.
"""

import asyncio
import os
import tempfile
from pathlib import Path
from typing import Any

import pytest
import pytest_asyncio
import yaml
from stepflow_orchestrator import OrchestratorConfig, StepflowOrchestrator
from stepflow_py import StepflowClient

from stepflow_langflow_integration.converter.translator import LangflowConverter


@pytest.fixture(scope="module")
def converter():
    """Create Langflow converter instance."""
    return LangflowConverter()


@pytest.fixture(scope="module")
def shared_config():
    """Create a shared Stepflow configuration with Langflow database for all tests.

    This configuration includes:
    - Langflow component server
    - Shared database for memory/session handling
    - No environment variable setup (API keys now passed via variables)

    Returns a tuple of (config_dict, config_path) where config_path is a stable
    temporary file that persists for the module scope.
    """
    # Create shared database path
    shared_db_path = Path(tempfile.gettempdir()) / "stepflow_langflow_shared_test.db"

    # Always initialize database (remove old file to ensure clean state)
    if shared_db_path.exists():
        shared_db_path.unlink()

    # Initialize Langflow database using their service
    original_db_url = os.environ.get("LANGFLOW_DATABASE_URL")
    os.environ["LANGFLOW_DATABASE_URL"] = f"sqlite:///{shared_db_path}"

    try:
        from langflow.services.utils import initialize_services, teardown_services
        from lfx.services.deps import get_db_service

        # Clear any existing service cache
        asyncio.run(teardown_services())

        # Re-initialize services and create database
        asyncio.run(initialize_services())
        db_service = get_db_service()
        assert db_service is not None, "Database service not available"
        db_service.reload_engine()
        asyncio.run(db_service.create_db_and_tables())
    finally:
        if original_db_url:
            os.environ["LANGFLOW_DATABASE_URL"] = original_db_url
        else:
            os.environ.pop("LANGFLOW_DATABASE_URL", None)

    # Get path to langflow integration directory
    current_dir = Path(__file__).parent.parent.parent

    # Load environment variables from .env file
    env_vars = dict(os.environ)
    try:
        from dotenv import load_dotenv

        env_path = current_dir / ".env"
        if env_path.exists():
            load_dotenv(env_path)
            env_vars = dict(os.environ)
    except ImportError:
        pass

    # Build plugin environment with AstraDB credentials if available
    plugin_env = {
        "LANGFLOW_DATABASE_URL": f"sqlite:///{shared_db_path}",
        "LANGFLOW_AUTO_LOGIN": "false",
    }

    # Add AstraDB credentials if available
    if "ASTRA_DB_API_ENDPOINT" in env_vars:
        plugin_env["ASTRA_DB_API_ENDPOINT"] = env_vars["ASTRA_DB_API_ENDPOINT"]
    if "ASTRA_DB_APPLICATION_TOKEN" in env_vars:
        plugin_env["ASTRA_DB_APPLICATION_TOKEN"] = env_vars[
            "ASTRA_DB_APPLICATION_TOKEN"
        ]

    # Add OpenAI API key for embedding model serialization
    # This is needed because when Embeddings objects are serialized and passed between
    # components, the TypeConverter resolves environment variable placeholders
    if "OPENAI_API_KEY" in env_vars:
        plugin_env["OPENAI_API_KEY"] = env_vars["OPENAI_API_KEY"]

    # Create configuration dictionary
    config_dict = {
        "plugins": {
            "builtin": {"type": "builtin"},
            "langflow": {
                "type": "stepflow",
                "command": "uv",
                "args": [
                    "--project",
                    str(current_dir),
                    "run",
                    "stepflow-langflow-server",
                ],
                "env": plugin_env,
            },
        },
        "routes": {
            "/langflow/{*component}": [{"plugin": "langflow"}],
            "/builtin/{*component}": [{"plugin": "builtin"}],
        },
        "stateStore": {"type": "inMemory"},
    }

    # Write config to stable temporary file (persists for module scope)
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yml", delete=False
    ) as config_file:
        yaml.dump(config_dict, config_file, default_flow_style=False, sort_keys=False)
        config_path = Path(config_file.name)

    # Return config dict and path
    yield config_dict, str(config_path)

    # Cleanup after all tests in module complete
    config_path.unlink(missing_ok=True)


@pytest_asyncio.fixture(scope="module", loop_scope="module")
async def stepflow_client(shared_config):
    """Start StepflowOrchestrator and return connected StepflowClient.

    The orchestrator and client are shared across all tests for performance.
    """
    config_dict, config_path = shared_config

    # Check that STEPFLOW_DEV_BINARY is set
    dev_binary = os.environ.get("STEPFLOW_DEV_BINARY")
    if not dev_binary:
        pytest.skip(
            "STEPFLOW_DEV_BINARY environment variable not set. "
            "Set it to the path of the stepflow-server binary."
        )

    # Create orchestrator config
    orch_config = OrchestratorConfig(
        config_path=Path(config_path),
        startup_timeout=60.0,
    )

    # Start orchestrator
    orchestrator = StepflowOrchestrator(orch_config)
    await orchestrator._start()

    # Create client
    client = StepflowClient.connect(orchestrator.url)

    yield client

    # Cleanup
    await client.close()
    await orchestrator._stop()


class TestExecutor:
    """Encapsulates flow execution logic for cleaner test code."""

    def __init__(
        self,
        converter: LangflowConverter,
        client: StepflowClient,
    ):
        self.converter = converter
        self.client = client

    async def execute_flow(
        self,
        flow_name: str,
        input_data: dict[str, Any],
        variables: dict[str, Any] | None = None,
        tweaks: dict[str, dict[str, Any]] | None = None,
        timeout: float = 60.0,
    ) -> dict[str, Any]:
        """Execute complete flow lifecycle: convert → store/validate → execute.

        Args:
            flow_name: Name of flow fixture (without .json extension)
            input_data: Input data for workflow execution
            variables: Variables to pass to the execution
            tweaks: Optional Stepflow-level tweaks to apply

        Returns:
            Execution result dict

        Raises:
            AssertionError: If any step fails
            pytest.skip: If dependencies not available
        """
        from stepflow_langflow_integration.converter.stepflow_tweaks import (
            convert_tweaks_to_overrides,
        )

        # Step 1: Load and convert
        langflow_data = load_flow_fixture(flow_name)
        stepflow_workflow = self.converter.convert(langflow_data)

        # Step 2: Store workflow (API validates and returns diagnostics)
        # Pass the Flow object directly to avoid dict conversion issues
        store_response = await self.client.store_flow(stepflow_workflow)

        # Check for validation errors
        if store_response.diagnostics.num_fatal > 0:
            diagnostics_list = [
                d.to_dict() for d in store_response.diagnostics.diagnostics
            ]
            num_fatal = store_response.diagnostics.num_fatal
            raise AssertionError(
                f"Workflow validation failed with {num_fatal} fatal errors:\n"
                f"{diagnostics_list}"
            )

        if not store_response.flow_id:
            diag_dict = store_response.diagnostics.to_dict()
            raise AssertionError(f"Failed to store workflow. Diagnostics: {diag_dict}")

        flow_id = store_response.flow_id

        # Step 3: Execute workflow with overrides (if provided)
        overrides = convert_tweaks_to_overrides(tweaks) if tweaks else None
        result_data = await self._execute_workflow(
            flow_id, input_data, variables, overrides, timeout
        )

        return result_data

    async def _execute_workflow(
        self,
        flow_id: str,
        input_data: dict,
        variables: dict | None = None,
        overrides: dict | None = None,
        timeout: float = 60.0,
    ) -> dict:
        """Execute workflow with optional overrides using StepflowClient."""
        # Submit the run
        response = await self.client.run(
            flow_id=flow_id,
            input_data=input_data,
            variables=variables,
            overrides=overrides,
            timeout=timeout,
        )

        result = response.to_dict()

        # Check if execution succeeded
        if result.get("status") != "completed":
            status = result.get("status")
            raise AssertionError(
                f"Workflow execution failed with status {status}. "
                f"Full response: {result}"
            )

        # Get the flow result and check its structure
        flow_result = result.get("result")
        if not flow_result:
            raise AssertionError("No result field in response")

        # Handle both direct result and wrapped result formats
        if isinstance(flow_result, dict) and flow_result.get("outcome") == "success":
            return flow_result
        elif isinstance(flow_result, dict) and "outcome" in flow_result:
            raise AssertionError(f"Workflow execution failed: {flow_result}")
        else:
            # For cases where flow_result is the actual result data
            return {"outcome": "success", "result": flow_result}


@pytest.fixture(scope="module")
def test_executor(converter, stepflow_client, shared_config):
    """Create a TestExecutor with all necessary dependencies."""
    return TestExecutor(converter, stepflow_client)


def load_flow_fixture(flow_name: str) -> dict[str, Any]:
    """Load Langflow JSON fixture by name."""
    import json

    fixtures_dir = Path(__file__).parent.parent / "fixtures" / "langflow"
    flow_path = fixtures_dir / f"{flow_name}.json"

    if not flow_path.exists():
        pytest.skip(f"Flow fixture not found: {flow_path}")

    with open(flow_path, encoding="utf-8") as f:
        return json.load(f)


# API-dependent flows


@pytest.mark.asyncio(loop_scope="module")
async def test_basic_prompting(test_executor):
    """Test basic prompting: custom Prompt + LanguageModelComponent with OpenAI API."""
    result = await test_executor.execute_flow(
        flow_name="basic_prompting",
        input_data={"message": "Write a haiku about testing"},
        timeout=60.0,
        variables={"OPENAI_API_KEY": os.environ.get("OPENAI_API_KEY", "")},
    )

    # Should return a Langflow Message with text content
    message_result = result["result"]
    assert isinstance(message_result, dict)
    # Check for both old and new Message serialization formats
    # Old: __langflow_type__, New (lfx): __class_name__ + __module_name__
    is_message = (
        "__langflow_type__" in message_result or "__class_name__" in message_result
    )
    assert is_message, f"Expected Message object, got: {message_result.keys()}"
    assert "text" in message_result
    # Haiku should have some structure (multiple lines)
    assert len(message_result["text"].split("\n")) >= 3


@pytest.mark.asyncio(loop_scope="module")
async def test_basic_prompting_api_key_from_env(test_executor):
    """Test basic prompting: custom Prompt + LanguageModelComponent with OpenAI API."""
    result = await test_executor.execute_flow(
        flow_name="basic_prompting",
        input_data={"message": "Write a haiku about testing"},
        timeout=60.0,
        variables={
            # By not providing the OPENAI_API_KEY as a variable we
            # are verifying that the UDF executor correctly loads it from the
            # environment.
            # "OPENAI_API_KEY": os.environ.get("OPENAI_API_KEY", "")
        },
    )

    # Should return a Langflow Message with text content
    message_result = result["result"]
    assert isinstance(message_result, dict)
    # Check for both old and new Message serialization formats
    # Old: __langflow_type__, New (lfx): __class_name__ + __module_name__
    is_message = (
        "__langflow_type__" in message_result or "__class_name__" in message_result
    )
    assert is_message, f"Expected Message object, got: {message_result.keys()}"
    assert "text" in message_result
    # Haiku should have some structure (multiple lines)
    assert len(message_result["text"].split("\n")) >= 3


@pytest.mark.asyncio(loop_scope="module")
async def test_vector_store_rag(test_executor):
    """Test vector store RAG: complex workflow with embeddings and retrieval."""
    # Create temporary test document for RAG processing
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
        # Configure tweaks for OpenAI and AstraDB
        from tests.helpers.tweaks_builder import TweaksBuilder

        tweaks = (
            TweaksBuilder()
            .add_astradb_tweaks("AstraDB-TCSqR")  # First AstraDB vector store
            .add_astradb_tweaks("AstraDB-BteL9")  # Second AstraDB vector store
            # Ingestion embeddings
            .add_env_tweak("OpenAIEmbeddings-jsaKm", "openai_api_key", "OPENAI_API_KEY")
            # Search embeddings
            .add_env_tweak("OpenAIEmbeddings-U8tZg", "openai_api_key", "OPENAI_API_KEY")
            .build_or_skip()
        )

        try:
            result = await test_executor.execute_flow(
                flow_name="vector_store_rag",
                input_data={
                    "message": "What is the main topic of the document?",
                    "file_path": test_file_path,  # Provide the file path
                },
                timeout=120.0,
                tweaks=tweaks,
                variables={
                    "OPENAI_API_KEY": os.environ.get("OPENAI_API_KEY", ""),
                    "ASTRA_DB_APPLICATION_TOKEN": os.environ.get(
                        "ASTRA_DB_APPLICATION_TOKEN", ""
                    ),
                },
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


@pytest.mark.asyncio(loop_scope="module")
async def test_memory_chatbot(test_executor, shared_config):
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
    from langflow.memory import astore_message
    from langflow.schema.message import Message

    # Setup test data with different session IDs upfront
    our_session_id = "test-memory-session-123"
    other_session_id = "other-session-456"

    # Phase 1: Pre-populate database with messages using proper Langflow objects
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

    # Access shared config dict via the server's config
    config_dict, config_path = shared_config
    langflow_plugin_config = config_dict["plugins"]["langflow"]
    db_url = langflow_plugin_config.get("env", {}).get("LANGFLOW_DATABASE_URL", "")
    # Extract path from sqlite:///path format
    db_path = (
        db_url.replace("sqlite:///", "") if db_url.startswith("sqlite:///") else None
    )

    assert db_path and Path(db_path).exists(), f"Database file not found at {db_path}"

    # Set up database environment for Langflow's storage methods
    # Temporarily clear any existing database URL to force re-initialization
    original_db_url = os.environ.get("LANGFLOW_DATABASE_URL")
    os.environ["LANGFLOW_DATABASE_URL"] = f"sqlite:///{db_path}"

    try:
        # Store messages using Langflow's proper async storage method
        import uuid

        test_flow_id = uuid.uuid4()  # Generate a proper UUID for flow_id

        await astore_message(alex_user_msg, flow_id=test_flow_id)
        await astore_message(alex_ai_msg, flow_id=test_flow_id)
        await astore_message(bob_user_msg, flow_id=test_flow_id)
        await astore_message(bob_ai_msg, flow_id=test_flow_id)
    finally:
        # Restore original database URL if it existed
        if original_db_url:
            os.environ["LANGFLOW_DATABASE_URL"] = original_db_url
        else:
            os.environ.pop("LANGFLOW_DATABASE_URL", None)

    # Phase 2: Test end-to-end memory functionality
    result = await test_executor.execute_flow(
        flow_name="memory_chatbot",
        input_data={"message": "What is my name?", "session_id": our_session_id},
        timeout=120.0,  # Increased timeout for memory operations
        variables={"OPENAI_API_KEY": os.environ.get("OPENAI_API_KEY", "")},
    )

    # Check for the known Langflow bug
    our_response = result["result"]["text"]

    # Test Bob's session
    result_other = await test_executor.execute_flow(
        flow_name="memory_chatbot",
        input_data={"message": "What is my name?", "session_id": other_session_id},
        timeout=120.0,  # Increased timeout for memory operations
        variables={"OPENAI_API_KEY": os.environ.get("OPENAI_API_KEY", "")},
    )

    bob_response = result_other["result"]["text"]

    # Verify session isolation: Each should reference their own name
    # Alex's session should know about Alex
    assert "alex" in our_response.lower(), (
        f"Alex's session should reference 'Alex'. Got: {our_response}"
    )

    # Bob's session should know about Bob
    assert "bob" in bob_response.lower(), (
        f"Bob's session should reference 'Bob'. Got: {bob_response}"
    )


@pytest.mark.asyncio(loop_scope="module")
async def test_document_qa(test_executor):
    """Test document Q&A: file parsing and question answering.

    This test validates:
    1. PDF parsing and text extraction via ParseData component
    2. OpenAI model invocation for question answering
    3. End-to-end document processing pipeline
    """
    # Create a test document
    test_content = """# Test Document

This is a test document about artificial intelligence and machine learning.

## Key Concepts

1. Machine Learning is a subset of AI
2. Deep Learning uses neural networks
3. Natural Language Processing handles text

## Applications

AI is used in many areas including:
- Healthcare diagnostics
- Financial analysis
- Autonomous vehicles
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
        f.write(test_content)
        test_file_path = f.name

    try:
        result = await test_executor.execute_flow(
            flow_name="document_qa",
            input_data={
                "file_path": test_file_path,
                "message": "What are the key concepts mentioned in the document?",
                "session_id": "test-document-qa-session",
            },
            timeout=90.0,
            variables={"OPENAI_API_KEY": os.environ.get("OPENAI_API_KEY", "")},
        )

        # Should return a response about the document
        message_result = result["result"]
        assert isinstance(message_result, dict)
        assert "text" in message_result

        # Response should reference document content
        response_text = message_result["text"].lower()
        content_indicators = [
            "machine learning",
            "deep learning",
            "neural",
            "ai",
            "artificial intelligence",
        ]
        found_content = any(
            indicator in response_text for indicator in content_indicators
        )
        assert found_content, (
            f"Response should reference document content. Got: {response_text}"
        )

    finally:
        os.unlink(test_file_path)


@pytest.mark.asyncio(loop_scope="module")
async def test_simple_agent(test_executor):
    """Test simple agent: tool-using agent with calculator.

    This test validates:
    1. Agent initialization and tool binding
    2. Tool invocation (calculator)
    3. Response generation with tool results
    """
    result = await test_executor.execute_flow(
        flow_name="simple_agent",
        input_data={"message": "What is 25 * 4?", "session_id": "test-agent-session"},
        timeout=90.0,
        variables={"OPENAI_API_KEY": os.environ.get("OPENAI_API_KEY", "")},
    )

    # Should return a response with the calculation result
    message_result = result["result"]
    assert isinstance(message_result, dict)
    assert "text" in message_result

    # Response should contain the answer (100)
    response_text = message_result["text"]
    assert "100" in response_text, (
        f"Response should contain '100' (25*4). Got: {response_text}"
    )
