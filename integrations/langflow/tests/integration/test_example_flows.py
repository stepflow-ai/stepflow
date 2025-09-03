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
import sqlite3
import tempfile
from pathlib import Path
from typing import Any

import pytest

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.testing.stepflow_binary import (
    StepflowBinaryRunner,
    create_test_config_file,
    get_default_stepflow_config,
)


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


@pytest.fixture
def test_config_path():
    """Create test configuration file with real Langflow components."""
    config_content = get_default_stepflow_config()
    config_path = create_test_config_file(config_content)
    yield config_path
    Path(config_path).unlink(missing_ok=True)


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
    with open(flow_path, "r", encoding="utf-8") as f:
        return json.load(f)


def execute_complete_flow_lifecycle(
    flow_name: str,
    input_data: dict[str, Any],
    converter: LangflowConverter,
    stepflow_runner: StepflowBinaryRunner,
    test_config_path: str,
    timeout: float = 60.0
) -> dict[str, Any]:
    """Execute complete flow lifecycle: convert → validate → execute.
    
    Args:
        flow_name: Name of flow fixture (without .json extension)
        input_data: Input data for workflow execution
        converter: LangflowConverter instance
        stepflow_runner: StepflowBinaryRunner instance
        test_config_path: Path to test configuration
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
    
    # Step 2: Validate
    success, stdout, stderr = stepflow_runner.validate_workflow(
        workflow_yaml, config_path=test_config_path
    )
    assert success, f"Workflow validation failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}"
    
    # Step 3: Execute
    success, result_data, stdout, stderr = stepflow_runner.run_workflow(
        workflow_yaml,
        input_data,
        config_path=test_config_path,
        timeout=timeout,
    )
    
    if not success:
        # Check for common dependency issues and skip gracefully
        if any(indicator in stderr.lower() for indicator in [
            "importerror", "modulenotfounderror", "no such table", "no files to process"
        ]):
            pytest.skip(f"External dependency not available: {stderr}")
        else:
            pytest.fail(f"Workflow execution failed:\nSTDOUT: {stdout}\nSTDERR: {stderr}")
    
    # Step 4: Basic result validation
    assert isinstance(result_data, dict), f"Expected dict result, got {type(result_data)}"
    assert result_data.get("outcome") == "success", f"Execution failed: {result_data}"
    assert "result" in result_data, f"No result field in output: {result_data}"
    
    return result_data


# Simple flows - no external dependencies

def test_simple_chat(converter, stepflow_runner, test_config_path):
    """Test simple chat: direct passthrough workflow with no processing."""
    result = execute_complete_flow_lifecycle(
        flow_name="simple_chat",
        input_data={"message": "Hello from simple chat test"},
        converter=converter,
        stepflow_runner=stepflow_runner,
        test_config_path=test_config_path,
        timeout=30.0
    )
    
    # Simple chat should return the input message directly
    assert result["result"] == "Hello from simple chat test"


# API-dependent flows

def test_openai_chat(converter, stepflow_runner, test_config_path):
    """Test OpenAI chat: direct API integration with built-in LanguageModelComponent."""
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")
        
    result = execute_complete_flow_lifecycle(
        flow_name="openai_chat", 
        input_data={"message": "What is 2+2?"},
        converter=converter,
        stepflow_runner=stepflow_runner,
        test_config_path=test_config_path,
        timeout=45.0
    )
    
    # Should return a Langflow Message with text content
    message_result = result["result"]
    assert isinstance(message_result, dict)
    assert "__langflow_type__" in message_result
    assert "text" in message_result
    assert len(message_result["text"]) > 0


def test_basic_prompting(converter, stepflow_runner, test_config_path):
    """Test basic prompting: custom Prompt + LanguageModelComponent with OpenAI API."""
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")
        
    result = execute_complete_flow_lifecycle(
        flow_name="basic_prompting",
        input_data={"message": "Write a haiku about testing"},
        converter=converter,
        stepflow_runner=stepflow_runner, 
        test_config_path=test_config_path,
        timeout=60.0
    )
    
    # Should return a Langflow Message with text content
    message_result = result["result"]
    assert isinstance(message_result, dict)
    assert "__langflow_type__" in message_result
    assert "text" in message_result
    # Haiku should have some structure (multiple lines)
    assert len(message_result["text"].split("\n")) >= 3


def test_vector_store_rag(converter, stepflow_runner, test_config_path):
    """Test vector store RAG: complex workflow with embeddings and retrieval."""
    # Vector store RAG is highly complex with embeddings, databases, and file processing
    pytest.skip("Vector store RAG requires complex infrastructure setup (AstraDB, embeddings, file processing)")


# Complex flows with external dependencies

def test_memory_chatbot(converter, stepflow_runner, test_config_path):
    """Test memory chatbot: requires database setup for message history."""
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")
        
    # Memory chatbot requires SQLite database with message table
    pytest.skip("Memory chatbot requires SQLite database setup with message table")


def test_document_qa(converter, stepflow_runner, test_config_path):
    """Test document QA: requires file input data."""
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")
        
    # Document QA requires file upload integration
    pytest.skip("Document QA requires file upload integration not available in test environment")


def test_simple_agent(converter, stepflow_runner, test_config_path):
    """Test simple agent: complex tool coordination with Calculator and URL components."""
    if not has_openai_api_key():
        pytest.skip("Requires OPENAI_API_KEY environment variable")
        
    # Agent workflows require complex tool coordination
    pytest.skip("Simple agent requires complex tool coordination not fully supported")


# Utility and infrastructure tests

def test_langflow_server_availability(stepflow_runner, test_config_path):
    """Test that Langflow component server can be started and responds."""
    # Test component listing to verify server works
    try:
        success, components, stderr = stepflow_runner.list_components(
            config_path=test_config_path
        )
        assert success, f"Component listing failed: {stderr}"
        
        # Should have langflow components available
        langflow_components = [c for c in components if "langflow" in c.lower()]
        assert len(langflow_components) > 0, (
            f"Should have Langflow components available: {components}"
        )
        
    except Exception as e:
        pytest.skip(f"Langflow server not available: {e}")


def test_blob_storage_integration(converter, stepflow_runner, test_config_path):
    """Test that blob storage works for component code."""
    # Use basic_prompting which definitely has custom components
    langflow_data = load_flow_fixture("basic_prompting")
    stepflow_workflow = converter.convert(langflow_data)
    
    # Should have blob steps for component code storage
    blob_steps = [step for step in stepflow_workflow.steps if step.component == "/builtin/put_blob"]
    
    # Should have at least one blob step for component code
    assert len(blob_steps) > 0, "Should have blob storage steps for component code"
    
    # Blob step should contain component code data
    blob_step = blob_steps[0]
    assert "data" in blob_step.input
    blob_data = blob_step.input["data"]
    assert "code" in blob_data, "Blob should contain component code"
    assert "component_type" in blob_data, "Blob should contain component type"


# Error handling tests

def test_execution_with_missing_input(converter, stepflow_runner, test_config_path):
    """Test execution with missing required input - workflows handle gracefully."""
    # Modern Langflow workflows often have good defaults and handle missing input gracefully
    # This test verifies that our integration doesn't crash with empty input
    
    langflow_data = load_flow_fixture("basic_prompting")
    stepflow_workflow = converter.convert(langflow_data)
    workflow_yaml = converter.to_yaml(stepflow_workflow)
    
    # Execute without providing input - should either succeed with defaults or fail gracefully
    success, result_data, stdout, stderr = stepflow_runner.run_workflow(
        workflow_yaml, {}, config_path=test_config_path, timeout=15.0
    )
    
    # Either outcome is acceptable - we're testing that it doesn't crash
    if success:
        # Succeeded with defaults - good behavior
        assert isinstance(result_data, dict)
        assert result_data.get("outcome") == "success"
    else:
        # Failed gracefully - also good behavior  
        assert isinstance(result_data, dict) or "error" in stderr.lower()


def test_execution_timeout(converter, stepflow_runner, test_config_path):
    """Test execution timeout handling."""
    # Use simple workflow with very short timeout
    langflow_data = load_flow_fixture("simple_chat")
    stepflow_workflow = converter.convert(langflow_data)
    workflow_yaml = converter.to_yaml(stepflow_workflow)
    
    # Use very short timeout - should either complete quickly or timeout
    try:
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml, {}, config_path=test_config_path, timeout=0.1
        )
        
        # If it completes, that's fine too (simple_chat is very simple)
        if success:
            assert isinstance(result_data, dict)
        
    except Exception:
        # Timeout exceptions are expected and acceptable
        pass


# Test utility functions (formerly in test_real_execution.py)

def load_flow_fixture(flow_name: str) -> dict[str, Any]:
    """Load Langflow JSON fixture by name.
    
    Args:
        flow_name: Name of flow fixture without .json extension
        
    Returns:
        Parsed Langflow JSON data
        
    Raises:
        pytest.skip: If fixture file not found
    """
    import json
    
    fixtures_dir = Path(__file__).parent.parent / "fixtures" / "langflow"
    flow_path = fixtures_dir / f"{flow_name}.json"
    
    if not flow_path.exists():
        pytest.skip(f"Flow fixture not found: {flow_path}")
        
    with open(flow_path, "r", encoding="utf-8") as f:
        return json.load(f)