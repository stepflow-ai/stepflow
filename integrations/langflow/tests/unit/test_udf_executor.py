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

"""Unit tests for UDFExecutor - test UDF execution in isolation with real Langflow code."""

import pytest
import asyncio
import json
from unittest.mock import Mock, AsyncMock
from typing import Dict, Any

from stepflow_langflow_integration.executor.udf_executor import UDFExecutor
from stepflow_langflow_integration.utils.errors import ExecutionError


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
                    "required": False
                }
            },
            "outputs": [
                {
                    "name": "result",
                    "method": "process_text",
                    "types": ["Message"]
                }
            ],
            "selected_output": "result"
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
from langflow.utils.constants import MESSAGE_SENDER_AI, MESSAGE_SENDER_USER, MESSAGE_SENDER_NAME_USER

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
                    "required": True
                },
                "sender": {
                    "type": "str", 
                    "value": "User",
                    "options": ["AI", "User"],
                    "info": "Type of sender"
                },
                "sender_name": {
                    "type": "str",
                    "value": "User",
                    "info": "Name of the sender"
                }
            },
            "outputs": [
                {
                    "name": "message", 
                    "method": "message_response",
                    "types": ["Message"]
                }
            ],
            "selected_output": "message"
        }

    @pytest.mark.asyncio
    async def test_execute_missing_blob_id(self, executor: UDFExecutor, mock_context):
        """Test execution fails when blob_id is missing."""
        input_data = {"input": {"text": "test"}}
        
        with pytest.raises(ExecutionError, match="No blob_id provided"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio 
    async def test_execute_simple_component(
        self, 
        executor: UDFExecutor, 
        mock_context,
        simple_component_blob: Dict[str, Any]
    ):
        """Test execution of a simple custom component."""
        # Setup mock context
        mock_context.get_blob.return_value = simple_component_blob
        
        # Input data with blob_id and runtime inputs
        input_data = {
            "blob_id": "test_blob_123",
            "input": {
                "text_input": "Hello World"
            }
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
        assert "text" in result_data
        assert result_data["text"] == "Processed: Hello World"
        assert result_data["sender"] == "SimpleTestComponent"
        assert result_data["sender_name"] == "Test Component"

    @pytest.mark.asyncio
    async def test_execute_chat_input_component(
        self, 
        executor: UDFExecutor,
        mock_context, 
        chat_input_component_blob: Dict[str, Any]
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
                "sender_name": "Test User"
            }
        }

        # Execute
        result = await executor.execute(input_data, mock_context)

        # Verify result
        assert "result" in result
        result_data = result["result"]
        
        # Should have Langflow Message structure
        assert "text" in result_data
        assert result_data["text"] == "What is Python?" 
        assert result_data["sender"] == "User"
        assert result_data["sender_name"] == "Test User"
        assert result_data.get("__langflow_type__") == "Message"

    @pytest.mark.asyncio
    async def test_execute_component_with_template_defaults(
        self,
        executor: UDFExecutor,
        mock_context,
        simple_component_blob: Dict[str, Any]
    ):
        """Test that template default values are used when runtime input is not provided."""
        # Modify blob to have default value
        simple_component_blob["template"]["text_input"]["value"] = "Default Text"
        mock_context.get_blob.return_value = simple_component_blob

        # Input data without runtime input for text_input
        input_data = {
            "blob_id": "test_blob_789",
            "input": {}  # No runtime inputs provided
        }

        # Execute
        result = await executor.execute(input_data, mock_context)

        # Should use template default
        result_data = result["result"] 
        assert result_data["text"] == "Processed: Default Text"

    @pytest.mark.asyncio
    async def test_execute_component_runtime_overrides_template(
        self,
        executor: UDFExecutor, 
        mock_context,
        simple_component_blob: Dict[str, Any]
    ):
        """Test that runtime inputs override template defaults."""
        # Set template default
        simple_component_blob["template"]["text_input"]["value"] = "Template Default"
        mock_context.get_blob.return_value = simple_component_blob

        # Input data with runtime override  
        input_data = {
            "blob_id": "test_blob_override",
            "input": {
                "text_input": "Runtime Override"
            }
        }

        # Execute
        result = await executor.execute(input_data, mock_context)

        # Should use runtime value, not template default
        result_data = result["result"]
        assert result_data["text"] == "Processed: Runtime Override"

    @pytest.mark.asyncio
    async def test_execute_component_with_missing_code(self, executor: UDFExecutor, mock_context):
        """Test execution fails when component code is missing."""
        blob_data = {
            "component_type": "TestComponent",
            "template": {},
            "outputs": []
            # Missing "code" field
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "missing_code_blob", "input": {}}

        with pytest.raises(ExecutionError, match="No code found for component"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio  
    async def test_execute_component_with_invalid_code(self, executor: UDFExecutor, mock_context):
        """Test execution fails when component code is invalid Python."""
        blob_data = {
            "code": "invalid python syntax !!!",
            "component_type": "InvalidComponent",
            "template": {},
            "outputs": []
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "invalid_code_blob", "input": {}}

        with pytest.raises(ExecutionError, match="Failed to execute component code"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_component_class_not_found(self, executor: UDFExecutor, mock_context):
        """Test execution fails when component class is not found in code."""
        blob_data = {
            "code": '''
# Valid Python code but no matching class
def some_function():
    return "not a component class"
''',
            "component_type": "MissingComponent",
            "template": {},
            "outputs": []
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "no_class_blob", "input": {}}

        with pytest.raises(ExecutionError, match="Component class MissingComponent not found"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_component_instantiation_fails(self, executor: UDFExecutor, mock_context):
        """Test execution fails when component cannot be instantiated."""
        blob_data = {
            "code": '''
class FailingComponent:
    def __init__(self):
        raise ValueError("Cannot instantiate this component")
''',
            "component_type": "FailingComponent",
            "template": {},
            "outputs": []
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "failing_init_blob", "input": {}}

        with pytest.raises(ExecutionError, match="Failed to instantiate FailingComponent"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_component_method_not_found(self, executor: UDFExecutor, mock_context):
        """Test execution fails when execution method is not found."""
        blob_data = {
            "code": '''
from langflow.custom.custom_component.component import Component

class NoMethodComponent(Component):
    pass
''',
            "component_type": "NoMethodComponent",
            "template": {},
            "outputs": [
                {"name": "result", "method": "nonexistent_method", "types": ["str"]}
            ],
            "selected_output": "result"
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "no_method_blob", "input": {}}

        with pytest.raises(ExecutionError, match="Method nonexistent_method not found"):
            await executor.execute(input_data, mock_context)

    @pytest.mark.asyncio
    async def test_execute_component_method_execution_fails(self, executor: UDFExecutor, mock_context):
        """Test execution fails when component method throws exception."""
        blob_data = {
            "code": '''
from langflow.custom.custom_component.component import Component

class FailingMethodComponent(Component):
    async def failing_method(self):
        raise RuntimeError("Method execution failed")
''',
            "component_type": "FailingMethodComponent",
            "template": {},
            "outputs": [
                {"name": "result", "method": "failing_method", "types": ["str"]}
            ],
            "selected_output": "result"
        }
        mock_context.get_blob.return_value = blob_data

        input_data = {"blob_id": "failing_method_blob", "input": {}}

        with pytest.raises(ExecutionError, match="Failed to execute failing_method"):
            await executor.execute(input_data, mock_context)

    def test_determine_environment_variable(self, executor: UDFExecutor):
        """Test environment variable determination from field configurations."""
        # Template string format
        assert executor._determine_environment_variable(
            "api_key", "${OPENAI_API_KEY}", {}
        ) == "OPENAI_API_KEY"

        # Direct env var name
        assert executor._determine_environment_variable(
            "token", "ANTHROPIC_API_KEY", {}
        ) == "ANTHROPIC_API_KEY"

        # Secret input field
        assert executor._determine_environment_variable(
            "api_key", "", {"_input_type": "SecretStrInput"}
        ) == "OPENAI_API_KEY"

        # OpenAI field name
        assert executor._determine_environment_variable(
            "openai_token", "", {"_input_type": "SecretStrInput"}
        ) == "OPENAI_API_KEY"

        # Generic secret field
        assert executor._determine_environment_variable(
            "custom_secret", "", {"_input_type": "SecretStrInput"}
        ) == "CUSTOM_SECRET"

        # No environment variable
        assert executor._determine_environment_variable(
            "normal_field", "normal_value", {}
        ) is None

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
    
    @pytest.fixture
    def registry(self):
        """Get test registry for real workflow components."""
        from ..integration.test_registry import get_test_registry
        return get_test_registry()
    
    @pytest.fixture 
    def converter(self):
        """Create converter instance."""
        from stepflow_langflow_integration.converter.translator import LangflowConverter
        return LangflowConverter()

    @pytest.mark.asyncio
    async def test_component_parameter_preparation(self, executor_with_mocks: UDFExecutor):
        """Test component parameter preparation logic."""
        template = {
            "text_field": {
                "type": "str",
                "value": "template_default",
                "info": "Text input field"
            },
            "api_key": {
                "type": "str", 
                "value": "${OPENAI_API_KEY}",
                "_input_type": "SecretStrInput"
            },
            "number_field": {
                "type": "int",
                "value": 42
            }
        }

        runtime_inputs = {
            "text_field": "runtime_override",
            "extra_field": "extra_value"
        }

        # Set environment variable for testing
        import os
        os.environ["OPENAI_API_KEY"] = "test-api-key-123"

        try:
            params = await executor_with_mocks._prepare_component_parameters(template, runtime_inputs)

            # Should have runtime override
            assert params["text_field"] == "runtime_override"
            
            # Should have environment variable substitution
            assert params["api_key"] == "test-api-key-123"
            
            # Should have template default
            assert params["number_field"] == 42
            
            # Should have runtime extra field
            assert params["extra_field"] == "extra_value"

        finally:
            # Clean up environment
            if "OPENAI_API_KEY" in os.environ:
                del os.environ["OPENAI_API_KEY"]

    def test_find_component_class(self, executor_with_mocks: UDFExecutor):
        """Test component class finding logic."""
        # Mock execution environment with classes
        exec_globals = {
            "TestComponent": type,
            "AnotherClass": str,
            "not_a_class": "string_value",
            "MyCustomComponent": type
        }

        # Direct match
        found_class = executor_with_mocks._find_component_class(exec_globals, "TestComponent")
        assert found_class == type

        # Case insensitive match
        found_class = executor_with_mocks._find_component_class(exec_globals, "testcomponent")
        assert found_class == type

        # Not found
        found_class = executor_with_mocks._find_component_class(exec_globals, "NonexistentComponent")
        assert found_class is None

        # Not a class
        found_class = executor_with_mocks._find_component_class(exec_globals, "not_a_class")
        assert found_class is None


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
    def registry(self):
        """Get test registry for real workflow components."""
        from ..integration.test_registry import get_test_registry
        return get_test_registry()
    
    @pytest.fixture 
    def converter(self):
        """Create converter instance."""
        from stepflow_langflow_integration.converter.translator import LangflowConverter
        return LangflowConverter()

    @pytest.mark.asyncio
    async def test_execute_real_chatinput_component_from_workflow(
        self,
        executor: UDFExecutor,
        mock_context,
        registry,
        converter
    ):
        """Test execution of ChatInput component with on-demand blob creation."""
        # Simulate a ChatInput component that doesn't exist in blob store
        blob_id = "udf_langflow_chatinput-abc123"
        mock_context.get_blob.side_effect = Exception("Blob not found")
        mock_context.put_blob.return_value = "generated_blob_id_123"
        
        input_data = {
            "blob_id": blob_id,
            "input": {
                "message": "Hello from test",
                "sender": "User"
            }
        }
        
        # Execute - should trigger on-demand blob creation for ChatInput
        result = await executor.execute(input_data, mock_context)
        
        # Verify blob creation was called
        mock_context.put_blob.assert_called_once()
        
        # Verify the generated blob contains proper ChatInput component code
        call_args = mock_context.put_blob.call_args[0][0]  # First positional argument
        assert "ChatInputComponent" in call_args["code"]
        assert "process_message" in call_args["code"]
        assert call_args["component_type"] == "ChatInputComponent"
        
        # Verify the actual execution result from the first call
        assert "result" in result
        result_data = result["result"]
        assert "text" in result_data
        assert result_data["text"] == "Hello from test"  # Should use the message we provided
        assert result_data["sender"] == "User"
        assert result_data["sender_name"] == "User"

    @pytest.mark.asyncio
    async def test_execute_converted_workflow_components(
        self,
        executor: UDFExecutor,
        mock_context,
        registry,
        converter
    ):
        """Test that components from converted workflows execute correctly."""
        # Load and convert a workflow
        workflow = registry.get_workflow_by_name('basic_prompting')
        langflow_data = registry.load_langflow_data(workflow)
        
        # Convert to find the UDF components
        stepflow_workflow = converter.convert(langflow_data)
        
        # Find steps that use /langflow/udf_executor
        udf_steps = [step for step in stepflow_workflow.steps 
                    if step.component == "/langflow/udf_executor"]
        
        assert len(udf_steps) > 0, "No UDF executor steps found in basic_prompting workflow"
        
        # Test the first UDF step
        first_step = udf_steps[0]
        
        # The step input should have blob_id reference
        assert "blob_id" in str(first_step.input), f"No blob_id found in step input: {first_step.input}"
        
        print(f"✅ Found {len(udf_steps)} UDF components in converted workflow")
        print(f"✅ First UDF step: {first_step.id} using {first_step.component}")

    @pytest.mark.asyncio
    async def test_execute_chatoutput_component_on_demand(
        self,
        executor: UDFExecutor,
        mock_context,
    ):
        """Test execution of ChatOutput component with on-demand blob creation."""
        # Simulate a ChatOutput component that doesn't exist in blob store
        blob_id = "udf_langflow_chatoutput-def456"
        mock_context.get_blob.side_effect = Exception("Blob not found")
        mock_context.put_blob.return_value = "generated_blob_id_456"
        
        # Create a mock input message from previous step
        input_message = {
            "text": "Mock AI response from language model",
            "sender": "AI",
            "sender_name": "Assistant", 
            "__langflow_type__": "Message"
        }

        input_data = {
            "blob_id": blob_id,
            "input": {
                "input_message": input_message
            }
        }

        # Execute - should trigger on-demand blob creation for ChatOutput
        result = await executor.execute(input_data, mock_context)
        
        # Verify blob creation was called
        mock_context.put_blob.assert_called_once()
        
        # Verify the generated blob contains proper ChatOutput component code
        call_args = mock_context.put_blob.call_args[0][0]  # First positional argument
        assert "ChatOutputComponent" in call_args["code"]
        assert "process_output" in call_args["code"]
        assert call_args["component_type"] == "ChatOutputComponent"
        
        # Verify result has proper structure
        assert "result" in result
        result_data = result["result"]
        assert "text" in result_data
        assert result_data["text"] == "Mock AI response from language model"

    @pytest.mark.asyncio
    @pytest.mark.skip(reason="Test uses outdated Langflow imports - needs updating to current Langflow API")
    async def test_real_component_code_execution_patterns(
        self,
        executor: UDFExecutor,
        mock_context,
        registry
    ):
        """Test execution patterns found in real Langflow component code."""
        # Create a blob with realistic OpenAI component code pattern
        openai_component_blob = {
            "code": '''
from langflow.base.langchain_utilities.model import LCModelComponent
from langflow.base.models.model import BaseModelComponent
from langflow.io import DropdownInput, FloatInput, IntInput, MessageInput, StrInput
from langflow.schema.message import Message


class OpenAIModelComponent(LCModelComponent, BaseModelComponent):
    display_name = "OpenAI"
    description = "Generates text using OpenAI LLMs."
    name = "OpenAIModel"

    inputs = [
        MessageInput(name="input_value", display_name="Input"),
        IntInput(
            name="max_tokens",
            display_name="Max Tokens",
            advanced=True,
            value=1000,
        ),
        DropdownInput(
            name="model_name",
            display_name="Model Name",
            advanced=False,
            options=["gpt-4", "gpt-3.5-turbo"],
            value="gpt-4",
        ),
        StrInput(
            name="openai_api_base",
            display_name="OpenAI API Base",
            advanced=True,
        ),
        StrInput(
            name="openai_api_key",
            display_name="OpenAI API Key",
            advanced=True,
        ),
        FloatInput(name="temperature", display_name="Temperature", value=0.1),
    ]

    async def build_model(self) -> Message:
        # Mock execution - return formatted response
        input_text = self.input_value.text if hasattr(self.input_value, 'text') else str(self.input_value)
        mock_response = f"OpenAI Mock Response for: {input_text}"
        
        return Message(
            text=mock_response,
            sender="OpenAI",
            sender_name="GPT Model"
        )
''',
            "component_type": "OpenAIModelComponent",
            "template": {
                "input_value": {"type": "Message", "required": True},
                "max_tokens": {"type": "int", "value": 1000},
                "model_name": {"type": "str", "value": "gpt-4"},
                "openai_api_base": {"type": "str", "value": ""},
                "openai_api_key": {"type": "str", "value": "${OPENAI_API_KEY}"},
                "temperature": {"type": "float", "value": 0.1}
            },
            "outputs": [
                {"name": "result", "method": "build_model", "types": ["Message"]}
            ],
            "selected_output": "result"
        }
        
        mock_context.get_blob.return_value = openai_component_blob
        
        # Create a mock input message
        input_message = {
            "text": "Write a haiku about testing",
            "sender": "User",
            "__langflow_type__": "Message"
        }
        
        input_data = {
            "blob_id": "openai_component_blob",
            "input": {
                "input_value": input_message,
                "temperature": 0.7,
                "max_tokens": 150
            }
        }
        
        # Execute
        result = await executor.execute(input_data, mock_context)
        
        # Verify execution
        assert "result" in result
        result_data = result["result"]
        assert "text" in result_data
        assert "OpenAI Mock Response" in result_data["text"]
        assert "Write a haiku about testing" in result_data["text"]
        assert result_data["sender"] == "OpenAI"