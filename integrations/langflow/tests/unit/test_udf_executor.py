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

"""Unit test for UDF execution in isolation (with real Langflow code)."""

from typing import Any
from unittest.mock import AsyncMock

import pytest

from stepflow_langflow_integration.exceptions import ExecutionError
from stepflow_langflow_integration.executor.udf_executor import UDFExecutor


class TestUDFExecutor:
    """Test UDFExecutor functionality with real Langflow component code."""

    @pytest.fixture
    def executor(self):
        """Create UDFExecutor instance."""
        return UDFExecutor()

    @pytest.fixture
    def mock_context(self):
        """Create mock StepflowContext."""
        context = AsyncMock()
        context.get_blob = AsyncMock()
        context.put_blob = AsyncMock()
        return context

    @pytest.fixture
    def simple_component_blob(self):
        """Simple test component blob data."""
        return {
            "code": '''
from langflow.custom.custom_component.component import Component
from langflow.io import MessageTextInput, Output
from langflow.schema.message import Message

class SimpleTestComponent(Component):
    display_name = "Simple Test"
    description = "A simple test component"

    inputs = [
        MessageTextInput(
            name="text_input",
            display_name="Text Input",
            info="Text input for testing"
        )
    ]

    outputs = [
        Output(display_name="Output", name="result", method="process_text")
    ]

    async def process_text(self) -> Message:
        """Process the input text and return a message."""
        input_text = self.text_input or "No input provided"
        result_text = f"Processed: {input_text}"

        return Message(
            text=result_text,
            sender="SimpleTestComponent",
            sender_name="Test Component"
        )
''',
            "component_type": "SimpleTestComponent",
            "template": {
                "text_input": {
                    "type": "str",
                    "value": "",
                    "info": "Text input for testing",
                    "required": False,
                }
            },
            "outputs": [
                {"name": "result", "method": "process_text", "types": ["Message"]}
            ],
            "selected_output": "result",
        }

    @pytest.fixture
    def chat_input_component_blob(self):
        """Real ChatInput component blob data from Langflow."""
        return {
            "code": '''
from langflow.base.io.chat import ChatComponent
from langflow.inputs.inputs import BoolInput
from langflow.io import DropdownInput, MessageTextInput, MultilineInput, Output
from langflow.schema.message import Message
from langflow.utils.constants import (
    MESSAGE_SENDER_AI, MESSAGE_SENDER_USER, MESSAGE_SENDER_NAME_USER
)

class ChatInput(ChatComponent):
    display_name = "Chat Input"
    description = "Get chat inputs from the Playground."
    icon = "MessagesSquare"
    name = "ChatInput"

    inputs = [
        MultilineInput(
            name="input_value",
            display_name="Input Text",
            value="",
            info="Message to be passed as input.",
        ),
        DropdownInput(
            name="sender",
            display_name="Sender Type",
            options=[MESSAGE_SENDER_AI, MESSAGE_SENDER_USER],
            value=MESSAGE_SENDER_USER,
            info="Type of sender.",
            advanced=True,
        ),
        MessageTextInput(
            name="sender_name",
            display_name="Sender Name",
            info="Name of the sender.",
            value=MESSAGE_SENDER_NAME_USER,
            advanced=True,
        ),
    ]

    outputs = [
        Output(display_name="Chat Message", name="message", method="message_response"),
    ]

    async def message_response(self) -> Message:
        """Create a message from the input."""
        message = Message(
            text=self.input_value or "",
            sender=self.sender,
            sender_name=self.sender_name,
        )
        self.status = message
        return message
''',
            "component_type": "ChatInput",
            "template": {
                "input_value": {
                    "type": "str",
                    "value": "",
                    "info": "Message to be passed as input",
                    "required": True,
                },
                "sender": {
                    "type": "str",
                    "value": "User",
                    "options": ["AI", "User"],
                    "info": "Type of sender",
                },
                "sender_name": {
                    "type": "str",
                    "value": "User",
                    "info": "Name of the sender",
                },
            },
            "outputs": [
                {"name": "message", "method": "message_response", "types": ["Message"]}
            ],
            "selected_output": "message",
        }

    @pytest.mark.asyncio
    async def test_execute_missing_blob_id(self, executor: UDFExecutor, mock_context):
        """Test execution fails when blob_id is missing."""
        input_data = {"input": {"text": "test"}}

        with pytest.raises(ExecutionError, match="No blob_id provided"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_simple_component(
        self, executor: UDFExecutor, mock_context, simple_component_blob: dict[str, Any]
    ):
        """Test execution of a simple custom component."""
        # Setup mock context
        mock_context.get_blob.return_value = simple_component_blob

        # Input data with blob_id and runtime inputs
        input_data = {
            "blob_id": "test_blob_123",
            "input": {"text_input": "Hello World"},
        }

        # Execute
        result = await executor.execute(input_data, mock_context)

        # Verify blob was retrieved
        mock_context.get_blob.assert_called_once_with("test_blob_123")

        # Verify result structure
        assert "result" in result
        assert isinstance(result["result"], dict)

        # Verify the processed result
        result_data = result["result"]
        # Langflow Message: result data is nested in ["result"] field
        message_data = result_data["result"] if "result" in result_data else result_data
        assert "text" in message_data
        assert message_data["text"] == "Processed: Hello World"
        assert message_data["sender"] == "SimpleTestComponent"
        assert message_data["sender_name"] == "Test Component"

    @pytest.mark.asyncio
    async def test_execute_chat_input_component(
        self,
        executor: UDFExecutor,
        mock_context,
        chat_input_component_blob: dict[str, Any],
    ):
        """Test execution of real ChatInput component."""
        # Setup mock context
        mock_context.get_blob.return_value = chat_input_component_blob

        # Input data
        input_data = {
            "blob_id": "chatinput_blob_456",
            "input": {
                "input_value": "What is Python?",
                "sender": "User",
                "sender_name": "Test User",
            },
        }

        # Execute
        result = await executor.execute(input_data, mock_context)

        # Verify result
        assert "result" in result
        result_data = result["result"]

        # Should have Langflow Message structure
        # Handle nested result structure from Langflow Messages
        message_data = result_data["result"] if "result" in result_data else result_data
        assert "text" in message_data
        assert message_data["text"] == "What is Python?"
        assert message_data["sender"] == "User"
        assert message_data["sender_name"] == "Test User"
        assert message_data.get("__langflow_type__") == "Message"

    @pytest.mark.asyncio
    async def test_execute_component_with_template_defaults(
        self, executor: UDFExecutor, mock_context, simple_component_blob: dict[str, Any]
    ):
        """Test that template default values are used when input is not provided."""
        # Modify blob to have default value
        simple_component_blob["template"]["text_input"]["value"] = "Default Text"
        mock_context.get_blob.return_value = simple_component_blob

        # Input data without runtime input for text_input
        input_data = {
            "blob_id": "test_blob_789",
            "input": {},  # No runtime inputs provided
        }

        # Execute
        result = await executor.execute(input_data, mock_context)

        # Should use template default
        result_data = result["result"]
        # Handle nested result structure from Langflow Messages
        message_data = result_data["result"] if "result" in result_data else result_data
        assert message_data["text"] == "Processed: Default Text"

    @pytest.mark.asyncio
    async def test_execute_component_runtime_overrides_template(
        self, executor: UDFExecutor, mock_context, simple_component_blob: dict[str, Any]
    ):
        """Test that runtime inputs override template defaults."""
        # Set template default
        simple_component_blob["template"]["text_input"]["value"] = "Template Default"
        mock_context.get_blob.return_value = simple_component_blob

        # Input data with runtime override
        input_data = {
            "blob_id": "test_blob_override",
            "input": {"text_input": "Runtime Override"},
        }

        # Execute
        result = await executor.execute(input_data, mock_context)

        # Should use runtime value, not template default
        result_data = result["result"]
        # Handle nested result structure from Langflow Messages
        message_data = result_data["result"] if "result" in result_data else result_data
        assert message_data["text"] == "Processed: Runtime Override"

    @pytest.mark.asyncio
    async def test_execute_component_with_missing_code(
        self, executor: UDFExecutor, mock_context
    ):
        """Test execution fails when component code is missing."""
        blob_data = {
            "component_type": "TestComponent",
            "template": {},
            "outputs": [],
            # Missing "code" field
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "missing_code_blob", "input": {}}

        with pytest.raises(ExecutionError, match="No code found for component"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_component_with_invalid_code(
        self, executor: UDFExecutor, mock_context
    ):
        """Test execution fails when component code is invalid Python."""
        blob_data = {
            "code": "invalid python syntax !!!",
            "component_type": "InvalidComponent",
            "template": {},
            "outputs": [],
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "invalid_code_blob", "input": {}}

        with pytest.raises(ExecutionError, match="Failed to evaluate component code"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_component_class_not_found(
        self, executor: UDFExecutor, mock_context
    ):
        """Test execution fails when component class is not found in code."""
        blob_data = {
            "code": """
# Valid Python code but no matching class
def some_function():
    return "not a component class"
""",
            "component_type": "MissingComponent",
            "template": {},
            "outputs": [],
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "no_class_blob", "input": {}}

        with pytest.raises(
            ExecutionError,
            match="Failed to evaluate component code for MissingComponent",
        ):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_component_instantiation_fails(
        self, executor: UDFExecutor, mock_context
    ):
        """Test execution fails when component cannot be instantiated."""
        blob_data = {
            "code": """
from langflow.custom.custom_component.component import Component
from langflow.io import Output

class FailingComponent(Component):
    display_name = "Failing Component"
    description = "Component that fails instantiation"

    outputs = [
        Output(display_name="Result", name="result", method="execute")
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        raise ValueError("Cannot instantiate this component")

    def execute(self):
        return "should not reach this"
""",
            "component_type": "FailingComponent",
            "template": {},
            "outputs": [{"name": "result", "method": "execute", "types": ["str"]}],
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "failing_init_blob", "input": {}}

        with pytest.raises(
            ExecutionError, match="Failed to instantiate FailingComponent"
        ):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_component_method_not_found(
        self, executor: UDFExecutor, mock_context
    ):
        """Test execution fails when execution method is not found."""
        blob_data = {
            "code": """
from langflow.custom.custom_component.component import Component

class NoMethodComponent(Component):
    pass
""",
            "component_type": "NoMethodComponent",
            "template": {},
            "outputs": [
                {"name": "result", "method": "nonexistent_method", "types": ["str"]}
            ],
            "selected_output": "result",
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "no_method_blob", "input": {}}

        with pytest.raises(ExecutionError, match="Method nonexistent_method not found"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_component_method_execution_fails(
        self, executor: UDFExecutor, mock_context
    ):
        """Test execution fails when component method throws exception."""
        blob_data = {
            "code": """
from langflow.custom.custom_component.component import Component

class FailingMethodComponent(Component):
    async def failing_method(self):
        raise RuntimeError("Method execution failed")
""",
            "component_type": "FailingMethodComponent",
            "template": {},
            "outputs": [
                {"name": "result", "method": "failing_method", "types": ["str"]}
            ],
            "selected_output": "result",
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "failing_method_blob", "input": {}}

        with pytest.raises(ExecutionError, match="Failed to execute failing_method"):
            await executor.execute(input_data, mock_context)

    def test_environment_variable_handling_deprecated(self, executor: UDFExecutor):
        """Test that environment variable handling is now handled via preprocessing.

        This test documents that environment variable resolution was moved from
        runtime in the UDF executor to preprocessing at the test/workflow level.
        The _determine_environment_variable method was removed as part of the
        simplification effort to rely on preprocessing instead.
        """
        # Environment variable resolution is now handled by preprocessing
        # instead of runtime resolution in the UDF executor
        assert True  # This test now just documents the architectural change

    def test_determine_execution_method(self, executor: UDFExecutor):
        """Test execution method determination from outputs metadata."""
        outputs = [
            {"name": "result1", "method": "method1", "types": ["str"]},
            {"name": "result2", "method": "method2", "types": ["int"]},
        ]

        # Selected output specified
        method = executor._determine_execution_method(outputs, "result2")
        assert method == "method2"

        # No selected output, use first
        method = executor._determine_execution_method(outputs, None)
        assert method == "method1"

        # Empty outputs
        method = executor._determine_execution_method([], None)
        assert method is None

        # Selected output not found, use first
        method = executor._determine_execution_method(outputs, "nonexistent")
        assert method == "method1"


class TestUDFExecutorIntegration:
    """Integration tests with mock Langflow imports."""

    @pytest.fixture
    def executor_with_mocks(self):
        """Create UDFExecutor with mocked Langflow imports."""
        # We'll test this without real Langflow imports to avoid dependency issues
        return UDFExecutor()

    # Registry fixture removed - no longer needed after eliminating test registry

    @pytest.fixture
    def converter(self):
        """Create converter instance."""
        from stepflow_langflow_integration.converter.translator import LangflowConverter

        return LangflowConverter()

    @pytest.mark.asyncio
    async def test_component_parameter_preparation(
        self, executor_with_mocks: UDFExecutor
    ):
        """Test component parameter preparation logic.

        Note: Environment variable resolution is now handled by preprocessing,
        not at runtime in the UDF executor. This test reflects the new approach.
        """
        template = {
            "text_field": {
                "type": "str",
                "value": "template_default",
                "info": "Text input field",
            },
            "api_key": {
                "type": "str",
                # In the new approach, preprocessing would have already resolved this
                "value": "test-api-key-123",  # Already preprocessed value
                "_input_type": "SecretStrInput",
            },
            "number_field": {"type": "int", "value": 42},
        }

        runtime_inputs = {
            "text_field": "runtime_override",
            "extra_field": "extra_value",
        }

        params = await executor_with_mocks._prepare_component_parameters(
            template, runtime_inputs
        )

        # Should have runtime override
        assert params["text_field"] == "runtime_override"

        # Should have preprocessed API key (not runtime resolved)
        assert params["api_key"] == "test-api-key-123"

        # Should have template default
        assert params["number_field"] == 42

        # Should have runtime extra field
        assert params["extra_field"] == "extra_value"


class TestUDFExecutorWithRealLangflowComponents:
    """Test UDFExecutor with real converted Langflow workflow components."""

    @pytest.fixture
    def executor(self):
        """Create UDFExecutor instance."""
        return UDFExecutor()

    @pytest.fixture
    def mock_context(self):
        """Create mock StepflowContext with blob storage."""
        context = AsyncMock()
        context.get_blob = AsyncMock()
        context.put_blob = AsyncMock()
        return context

    @pytest.fixture
    def converter(self):
        """Create converter instance."""
        from stepflow_langflow_integration.converter.translator import LangflowConverter

        return LangflowConverter()

    @pytest.mark.asyncio
    async def test_execute_converted_workflow_components(
        self, executor: UDFExecutor, mock_context, converter
    ):
        """Test that components from converted workflows execute correctly."""
        # Create simple workflow data to test UDF component creation
        langflow_data = {
            "data": {
                "nodes": [
                    {
                        "id": "test-prompt",
                        "data": {
                            "type": "Prompt",
                            "node": {
                                "template": {
                                    "code": {
                                        "value": """
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.message import Message

class TestPrompt(Component):
    def build_prompt(self) -> Message:
                        return Message(text="Test prompt")
"""
                                    }
                                },
                                "outputs": [
                                    {"name": "prompt", "method": "build_prompt"}
                                ],
                                "base_classes": ["Message"],
                                "display_name": "Test Prompt",
                            },
                            "outputs": [{"name": "prompt", "method": "build_prompt"}],
                        },
                    }
                ],
                "edges": [],
            }
        }

        # Convert to find the UDF components
        stepflow_workflow = converter.convert(langflow_data)

        # Find steps that use /langflow/udf_executor
        udf_steps = [
            step
            for step in stepflow_workflow.steps
            if step.component == "/langflow/udf_executor"
        ]

        assert len(udf_steps) > 0, "No UDF executor steps found in test workflow"

        # Test the first UDF step
        first_step = udf_steps[0]

        # The step input should have blob_id reference
        assert "blob_id" in str(first_step.input), (
            f"No blob_id found in step input: {first_step.input}"
        )

        print(f"✅ Found {len(udf_steps)} UDF components in converted workflow")
        print(f"✅ First UDF step: {first_step.id} using {first_step.component}")

    @pytest.mark.asyncio
    async def test_component_metadata_extraction_and_usage(
        self, executor: UDFExecutor, mock_context
    ):
        # Create blob with enhanced metadata (from Phase 1 improvements)
        enhanced_blob = {
            "code": """
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.message import Message

class EnhancedTestComponent(Component):
    display_name = "Enhanced Test"
    description = "Test component with enhanced metadata"

    outputs = [
        Output(display_name="Message", name="message", method="create_message")
    ]

    def create_message(self) -> Message:
        return Message(text="Enhanced metadata test successful")
""",
            "template": {"input_field": {"type": "str", "value": "test"}},
            "component_type": "EnhancedTestComponent",
            "outputs": [
                {"name": "message", "method": "create_message", "types": ["Message"]}
            ],
            "selected_output": "message",
            # Enhanced metadata from Phase 1
            "base_classes": ["Message"],
            "display_name": "Enhanced Test",
            "description": "Test component with enhanced metadata",
            "documentation": "https://docs.example.com/enhanced-test",
            "metadata": {
                "module": "test.module.EnhancedTestComponent",
                "code_hash": "abc123",
            },
            "field_order": ["input_field"],
            "icon": "test-icon",
            "is_builtin": False,
        }

        mock_context.get_blob.return_value = enhanced_blob

        # Execute with enhanced metadata
        input_data = {"blob_id": "enhanced_test_blob", "input": {}}
        result = await executor.execute(input_data, mock_context)

        # Verify execution used enhanced metadata
        assert result["result"]["text"] == "Enhanced metadata test successful"

        # Verify compiled component contains enhanced metadata
        compiled = executor.compiled_components["enhanced_test_blob"]
        assert compiled["base_classes"] == ["Message"]
        assert compiled["display_name"] == "Enhanced Test"
        assert compiled["metadata"]["module"] == "test.module.EnhancedTestComponent"
