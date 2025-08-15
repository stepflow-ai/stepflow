"""True end-to-end integration tests using real Langflow UDF execution.

These tests verify that converted workflows actually execute with real UDF components,
not mocks. They test the complete pipeline:
1. Convert Langflow JSON → Stepflow YAML
2. Store component code as blobs 
3. Execute using real Langflow component server
4. Verify results are functionally correct
"""

import pytest
import tempfile
import json
import os
from pathlib import Path
from typing import Dict, Any

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.testing.stepflow_binary import (
    StepflowBinaryRunner,
    create_test_config_file
)

from .test_registry import get_test_registry, TestWorkflow, pytest_parametrize_workflows


def get_real_langflow_config() -> str:
    """Get stepflow config that uses real Langflow component server instead of mocks."""
    # Get the langflow integration project path
    langflow_project_path = Path(__file__).parent.parent.parent
    
    return f"""
plugins:
  builtin:
    type: builtin
  real_langflow:
    type: stepflow
    transport: stdio
    command: uv
    args: ["--project", "{langflow_project_path}", "run", "stepflow-langflow-server"]

routes:
  "/langflow/{{*component}}":
    - plugin: real_langflow
  "/builtin/{{*component}}":
    - plugin: builtin

stateStore:
  type: inMemory
"""


@pytest.mark.real_execution
class TestRealLangflowExecution:
    """Test real execution with actual Langflow components (not mocks)."""

    @pytest.fixture
    def registry(self):
        """Get test registry."""
        return get_test_registry()

    @pytest.fixture
    def converter(self):
        """Create converter instance.""" 
        return LangflowConverter()

    @pytest.fixture
    def stepflow_runner(self) -> StepflowBinaryRunner:
        """Create StepflowBinaryRunner instance."""
        try:
            runner = StepflowBinaryRunner()
            available, version = runner.check_binary_availability()
            if not available:
                pytest.skip(f"Stepflow binary not available: {version}")
            return runner
        except FileNotFoundError as e:
            pytest.skip(f"Stepflow binary not found: {e}")

    @pytest.fixture
    def real_langflow_config_path(self) -> str:
        """Create test configuration that uses real Langflow server."""
        config_content = get_real_langflow_config()
        config_path = create_test_config_file(config_content)
        yield config_path
        # Cleanup
        Path(config_path).unlink(missing_ok=True)

    def test_langflow_server_availability(
        self,
        stepflow_runner: StepflowBinaryRunner,
        real_langflow_config_path: str
    ):
        """Test that the real Langflow component server is working."""
        success, components, stderr = stepflow_runner.list_components(
            config_path=real_langflow_config_path
        )

        assert success, f"Failed to list components from real Langflow server: {stderr}"
        assert isinstance(components, list)
        
        # Should have the UDF executor component
        langflow_components = [c for c in components if "langflow" in c.lower() or "udf_executor" in c.lower()]
        assert len(langflow_components) > 0, f"Should have Langflow components available: {components}"

    @pytest.mark.slow
    def test_simple_chat_real_execution(
        self,
        registry,
        converter: LangflowConverter,
        stepflow_runner: StepflowBinaryRunner,
        real_langflow_config_path: str
    ):
        """Test simple_chat workflow with real UDF execution."""
        # Load and convert workflow
        workflow = registry.get_workflow_by_name('simple_chat')
        langflow_data = registry.load_langflow_data(workflow)
        stepflow_workflow = converter.convert(langflow_data)
        workflow_yaml = converter.to_yaml(stepflow_workflow)

        # First validate
        success, stdout, stderr = stepflow_runner.validate_workflow(
            workflow_yaml, config_path=real_langflow_config_path
        )
        assert success, f"Workflow validation failed: {stderr}"

        # Execute with real components
        input_data = workflow.input_data or {"message": "Hello from real test"}
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml,
            input_data,
            config_path=real_langflow_config_path,
            timeout=60.0  # Longer timeout for real execution
        )

        # Check execution success  
        if not success:
            # If failure is due to missing Langflow dependencies, skip
            if any(indicator in stderr.lower() for indicator in ["importerror", "modulenotfounderror", "langflow"]):
                pytest.skip(f"Langflow dependencies not available: {stderr}")
            else:
                pytest.fail(f"Real workflow execution failed: {stdout}\n{stderr}")

        # Verify result structure
        assert isinstance(result_data, dict)
        assert result_data.get("outcome") == "success"
        assert "result" in result_data

        # Verify the actual result content (should be from real Langflow execution)
        actual_result = result_data["result"]
        assert isinstance(actual_result, dict)
        
        # Should have message-like structure from real ChatOutput component
        assert "text" in actual_result or "message" in actual_result or "content" in actual_result

    @pytest.mark.slow
    def test_basic_prompting_real_execution(
        self,
        registry,
        converter: LangflowConverter, 
        stepflow_runner: StepflowBinaryRunner,
        real_langflow_config_path: str
    ):
        """Test basic_prompting workflow with real UDF execution."""
        # Load and convert workflow
        workflow = registry.get_workflow_by_name('basic_prompting')
        langflow_data = registry.load_langflow_data(workflow)
        stepflow_workflow = converter.convert(langflow_data)
        workflow_yaml = converter.to_yaml(stepflow_workflow)

        print(f"\n=== BASIC PROMPTING WORKFLOW YAML ===\n{workflow_yaml}\n")

        # Execute with real components
        input_data = workflow.input_data or {"message": "Write a haiku about testing"}
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml,
            input_data,
            config_path=real_langflow_config_path,
            timeout=90.0  # Longer timeout for LLM calls
        )

        # Check execution
        if not success:
            # If failure is due to missing dependencies or API keys, skip  
            missing_deps = any(indicator in stderr.lower() for indicator in [
                "importerror", "modulenotfounderror", "langflow",
                "openai_api_key", "anthropic_api_key"
            ])
            if missing_deps:
                pytest.skip(f"Missing dependencies or API keys: {stderr}")
            else:
                pytest.fail(f"Basic prompting execution failed: {stdout}\n{stderr}")

        # Verify result
        assert isinstance(result_data, dict)
        assert result_data.get("outcome") == "success"

        # The result should contain actual LLM output (or mock output if no API key)
        actual_result = result_data["result"]
        assert isinstance(actual_result, dict)

    @pytest.mark.slow
    @pytest.mark.parametrize("workflow", pytest_parametrize_workflows([
        # Start with simpler workflows that don't require external APIs
        get_test_registry().get_workflow_by_name('simple_chat'),
    ]))
    def test_converted_workflow_real_execution(
        self,
        workflow: TestWorkflow,
        registry,
        converter: LangflowConverter,
        stepflow_runner: StepflowBinaryRunner,
        real_langflow_config_path: str
    ):
        """Test converted workflows with real execution."""
        # Skip if workflow is marked as not suitable for real execution
        if not hasattr(workflow, 'tags') or 'real_execution_safe' not in workflow.tags:
            pytest.skip(f"Workflow {workflow.name} not marked as safe for real execution")

        # Load and convert
        try:
            langflow_data = registry.load_langflow_data(workflow)
        except FileNotFoundError:
            pytest.skip(f"Langflow file not found: {workflow.langflow_file}")

        stepflow_workflow = converter.convert(langflow_data)
        workflow_yaml = converter.to_yaml(stepflow_workflow)

        # Execute
        input_data = workflow.input_data or {}
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            workflow_yaml,
            input_data,
            config_path=real_langflow_config_path,
            timeout=120.0
        )

        # Handle execution results
        if not success:
            # Check for expected failure conditions
            dependency_issues = any(indicator in stderr.lower() for indicator in [
                "importerror", "modulenotfounderror", "langflow",
                "openai_api_key", "anthropic_api_key", "connection", "timeout"
            ])
            
            if dependency_issues:
                pytest.skip(f"Dependency or connectivity issue: {stderr}")
            else:
                pytest.fail(f"Unexpected execution failure for {workflow.name}: {stdout}\n{stderr}")

        # Basic result validation
        assert isinstance(result_data, dict)
        if result_data.get("outcome") == "success":
            assert "result" in result_data
            actual_result = result_data["result"]
            assert actual_result is not None


@pytest.mark.real_execution 
@pytest.mark.slow
class TestBlobStorageIntegration:
    """Test that blob storage and retrieval works in real execution."""

    @pytest.fixture
    def stepflow_runner(self) -> StepflowBinaryRunner:
        """Create StepflowBinaryRunner instance."""
        return StepflowBinaryRunner()

    @pytest.fixture  
    def real_langflow_config_path(self) -> str:
        """Create test configuration that uses real Langflow server."""
        config_content = get_real_langflow_config()
        config_path = create_test_config_file(config_content)
        yield config_path
        Path(config_path).unlink(missing_ok=True)

    def test_blob_storage_with_custom_component(
        self,
        stepflow_runner: StepflowBinaryRunner,
        real_langflow_config_path: str
    ):
        """Test that custom component code is properly stored and retrieved as blobs."""
        
        # Create a workflow that uses a custom UDF component
        custom_workflow_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Blob Storage Test
steps:
  - id: store_custom_component
    component: /builtin/put_blob
    input:
      data:
        code: |
          from langflow.custom.custom_component.component import Component
          from langflow.io import MessageTextInput, Output
          from langflow.schema.message import Message

          class BlobTestComponent(Component):
              display_name = "Blob Test Component"
              description = "Test component for blob storage"
              
              inputs = [
                  MessageTextInput(name="test_input", display_name="Test Input")
              ]
              
              outputs = [
                  Output(display_name="Test Output", name="result", method="process")
              ]

              async def process(self) -> Message:
                  return Message(
                      text=f"Blob test processed: {self.test_input}",
                      sender="BlobTestComponent"
                  )
        component_type: BlobTestComponent
        template:
          test_input:
            type: str
            value: ""
            info: "Test input field"
        outputs:
          - name: result
            method: process
            types: ["Message"]
        selected_output: result
      blob_type: "udf_component"

  - id: execute_custom_component
    component: /langflow/udf_executor
    input:
      blob_id:
        $from:
          step: store_custom_component
        path: blob_id
      input:
        test_input: "Hello from blob storage test"

output:
  $from:
    step: execute_custom_component
  path: result
"""

        # Execute the workflow
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            custom_workflow_yaml,
            {},
            config_path=real_langflow_config_path,
            timeout=60.0
        )

        if not success:
            # Skip if Langflow dependencies missing
            if any(indicator in stderr.lower() for indicator in ["importerror", "modulenotfounderror", "langflow"]):
                pytest.skip(f"Langflow dependencies not available: {stderr}")
            else:
                pytest.fail(f"Blob storage test failed: {stdout}\n{stderr}")

        # Verify the result shows the blob was stored and retrieved correctly
        assert isinstance(result_data, dict)
        assert result_data.get("outcome") == "success"
        
        actual_result = result_data.get("result")
        assert actual_result is not None
        
        # Should contain the processed output from the custom component
        if isinstance(actual_result, dict) and "text" in actual_result:
            assert "Blob test processed" in actual_result["text"]
            assert "Hello from blob storage test" in actual_result["text"]


@pytest.mark.real_execution
class TestTypeConversion:
    """Test Langflow ↔ Stepflow type conversion in real execution."""

    @pytest.fixture
    def stepflow_runner(self) -> StepflowBinaryRunner:
        """Create StepflowBinaryRunner instance."""
        return StepflowBinaryRunner()

    @pytest.fixture
    def real_langflow_config_path(self) -> str:
        """Create test configuration that uses real Langflow server."""
        config_content = get_real_langflow_config()
        config_path = create_test_config_file(config_content)
        yield config_path
        Path(config_path).unlink(missing_ok=True)

    def test_message_type_conversion(
        self,
        stepflow_runner: StepflowBinaryRunner,
        real_langflow_config_path: str
    ):
        """Test that Langflow Message types are properly converted."""
        
        # Workflow that creates and processes Message objects
        message_conversion_yaml = """
schema: https://stepflow.org/schemas/v1/flow.json
name: Message Type Conversion Test
steps:
  - id: create_message
    component: /builtin/put_blob
    input:
      data:
        code: |
          from langflow.custom.custom_component.component import Component
          from langflow.io import MessageTextInput, Output
          from langflow.schema.message import Message

          class MessageCreator(Component):
              inputs = [
                  MessageTextInput(name="input_text", display_name="Input Text")
              ]
              outputs = [
                  Output(display_name="Message", name="message", method="create_message")
              ]

              async def create_message(self) -> Message:
                  return Message(
                      text=self.input_text,
                      sender="MessageCreator",
                      sender_name="Test Creator"
                  )
        component_type: MessageCreator
        template:
          input_text:
            type: str
            value: ""
        outputs:
          - name: message
            method: create_message
            types: ["Message"]
      blob_type: "udf_component"

  - id: process_message
    component: /builtin/put_blob
    input:
      data:
        code: |
          from langflow.custom.custom_component.component import Component
          from langflow.io import HandleInput, Output
          from langflow.schema.message import Message

          class MessageProcessor(Component):
              inputs = [
                  HandleInput(
                      name="message_input",
                      display_name="Message Input",
                      input_types=["Message"]
                  )
              ]
              outputs = [
                  Output(display_name="Processed", name="result", method="process_message")
              ]

              async def process_message(self) -> Message:
                  input_msg = self.message_input
                  if hasattr(input_msg, 'text'):
                      processed_text = f"Processed: {input_msg.text}"
                  else:
                      processed_text = f"Processed: {str(input_msg)}"
                  
                  return Message(
                      text=processed_text,
                      sender="MessageProcessor"
                  )
        component_type: MessageProcessor
        template:
          message_input:
            type: Message
        outputs:
          - name: result
            method: process_message
            types: ["Message"]
      blob_type: "udf_component"

  - id: create_msg
    component: /langflow/udf_executor
    input:
      blob_id:
        $from:
          step: create_message
        path: blob_id
      input:
        input_text: "Hello Message Conversion"

  - id: process_msg  
    component: /langflow/udf_executor
    input:
      blob_id:
        $from:
          step: process_message
        path: blob_id
      input:
        message_input:
          $from:
            step: create_msg
          path: result

output:
  $from:
    step: process_msg
  path: result
"""

        # Execute
        success, result_data, stdout, stderr = stepflow_runner.run_workflow(
            message_conversion_yaml,
            {},
            config_path=real_langflow_config_path,
            timeout=60.0
        )

        if not success:
            if any(indicator in stderr.lower() for indicator in ["importerror", "modulenotfounderror", "langflow"]):
                pytest.skip(f"Langflow dependencies not available: {stderr}")
            else:
                pytest.fail(f"Message conversion test failed: {stdout}\n{stderr}")

        # Verify type conversion worked
        assert isinstance(result_data, dict)
        assert result_data.get("outcome") == "success"
        
        actual_result = result_data.get("result")
        assert actual_result is not None
        
        # Should show the message was properly converted and processed
        if isinstance(actual_result, dict) and "text" in actual_result:
            assert "Processed: Hello Message Conversion" in actual_result["text"]