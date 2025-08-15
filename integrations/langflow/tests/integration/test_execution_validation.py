"""Integration tests for validating workflow execution results."""

import json
import asyncio
from pathlib import Path
from typing import Dict, Any
from unittest.mock import AsyncMock, patch, MagicMock

import pytest
import yaml

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.executor.udf_executor import UDFExecutor
from stepflow_langflow_integration.executor.type_converter import TypeConverter


class MockStepflowContext:
    """Mock StepflowContext for testing."""
    
    def __init__(self):
        self.blobs = {}
        self.blob_counter = 0
    
    async def put_blob(self, data: Dict[str, Any]) -> str:
        """Store blob data and return ID."""
        self.blob_counter += 1
        blob_id = f"test_blob_{self.blob_counter}"
        self.blobs[blob_id] = data
        return blob_id
    
    async def get_blob(self, blob_id: str) -> Dict[str, Any]:
        """Retrieve blob data by ID."""
        if blob_id not in self.blobs:
            raise ValueError(f"Blob not found: {blob_id}")
        return self.blobs[blob_id]


class TestExecutionValidation:
    """Test execution validation with various scenarios."""
    
    @pytest.fixture
    def mock_context(self) -> MockStepflowContext:
        """Create mock Stepflow context."""
        return MockStepflowContext()
    
    @pytest.fixture
    def udf_executor(self) -> UDFExecutor:
        """Create UDF executor for testing."""
        return UDFExecutor()
    
    @pytest.fixture
    def type_converter(self) -> TypeConverter:
        """Create type converter for testing."""
        return TypeConverter()
    
    @pytest.mark.asyncio
    async def test_simple_message_flow(
        self, 
        mock_context: MockStepflowContext,
        udf_executor: UDFExecutor
    ):
        """Test simple message flow with mocked components."""
        
        # Create UDF blob for ChatInput component
        chatinput_code = '''
from langflow.schema.message import Message
from langflow.custom.custom_component.component import Component

class ChatInput(Component):
    def __init__(self):
        super().__init__()
        self._parameters = {}
    
    def set_attributes(self, params):
        self._parameters = params
    
    async def message_response(self):
        input_text = self._parameters.get('input_value', 'Hello')
        return Message(
            text=input_text,
            sender='User',
            sender_name='User'
        )
'''
        
        chatinput_blob = {
            "code": chatinput_code,
            "template": {
                "input_value": {
                    "type": "str",
                    "value": "",
                    "info": "Input message"
                }
            },
            "component_type": "ChatInput",
            "outputs": [
                {
                    "name": "message",
                    "method": "message_response",
                    "types": ["Message"]
                }
            ],
            "selected_output": "message"
        }
        
        blob_id = await mock_context.put_blob(chatinput_blob)
        
        # Test execution with mocked Langflow imports
        with patch('stepflow_langflow_integration.executor.udf_executor.exec') as mock_exec:
            # Mock successful code execution
            def mock_exec_func(code, globals_dict):
                # Simulate adding the ChatInput class to globals
                class MockChatInput:
                    def __init__(self):
                        self._parameters = {}
                    
                    def set_attributes(self, params):
                        self._parameters = params
                    
                    async def message_response(self):
                        from langflow.schema.message import Message
                        input_text = self._parameters.get('input_value', 'Hello')
                        return Message(
                            text=input_text,
                            sender='User',
                            sender_name='User'
                        )
                
                globals_dict['ChatInput'] = MockChatInput
            
            mock_exec.side_effect = mock_exec_func
            
            # Execute UDF
            input_data = {
                "blob_id": blob_id,
                "input": {
                    "input_value": "Test message"
                }
            }
            
            result = await udf_executor.execute(input_data, mock_context)
            
            # Validate result structure
            assert "result" in result
            result_data = result["result"]
            
            # Should have Langflow Message structure
            assert result_data["text"] == "Test message"
            assert result_data["sender"] == "User"
            assert result_data["__langflow_type__"] == "Message"
    
    @pytest.mark.asyncio
    async def test_component_chain_execution(
        self,
        mock_context: MockStepflowContext, 
        udf_executor: UDFExecutor
    ):
        """Test chaining multiple components together."""
        
        # Create first component (input)
        input_blob = await self._create_mock_input_component(mock_context)
        
        # Create second component (processor)
        processor_blob = await self._create_mock_processor_component(mock_context)
        
        # Execute input component
        input_result = await udf_executor.execute(
            {
                "blob_id": input_blob,
                "input": {"input_value": "Raw input"}
            },
            mock_context
        )
        
        # Execute processor component with input result
        processor_result = await udf_executor.execute(
            {
                "blob_id": processor_blob,
                "input": {"input_data": input_result["result"]}
            },
            mock_context
        )
        
        # Validate chained execution
        assert "result" in processor_result
        processed_data = processor_result["result"]
        assert processed_data["text"] == "PROCESSED: Raw input"
    
    @pytest.mark.slow
    @pytest.mark.asyncio
    async def test_error_handling_scenarios(
        self,
        mock_context: MockStepflowContext,
        udf_executor: UDFExecutor
    ):
        """Test various error scenarios."""
        
        # Test missing blob
        with pytest.raises(Exception, match="No blob_id provided"):
            await udf_executor.execute({"input": {}}, mock_context)
        
        # Test invalid blob ID
        with pytest.raises(Exception, match="Blob not found"):
            await udf_executor.execute(
                {"blob_id": "nonexistent", "input": {}}, 
                mock_context
            )
        
        # Test component with syntax error
        bad_code_blob = {
            "code": "class BadComponent:\n    invalid syntax here",
            "component_type": "BadComponent", 
            "template": {},
            "outputs": [{"name": "out", "method": "run", "types": ["Data"]}],
            "selected_output": "out"
        }
        
        bad_blob_id = await mock_context.put_blob(bad_code_blob)
        
        with pytest.raises(Exception, match="Failed to execute component code"):
            await udf_executor.execute(
                {"blob_id": bad_blob_id, "input": {}},
                mock_context
            )
    
    @pytest.mark.asyncio
    async def test_type_conversion_accuracy(self, type_converter: TypeConverter):
        """Test type conversion between Langflow and Stepflow formats."""
        
        # Test Message serialization/deserialization with real Langflow types
        try:
            from langflow.schema.message import Message
            
            # Create real Message instance
            message = Message(
                text="Test",
                sender="User",
                sender_name="User"
            )
            
            # Test serialization
            serialized = type_converter.serialize_langflow_object(message)
            
            assert serialized["__langflow_type__"] == "Message"
            assert serialized["text"] == "Test"
            assert serialized["sender"] == "User"
            assert serialized["sender_name"] == "User"
            
            # Test deserialization
            deserialized = type_converter.deserialize_to_langflow_type(serialized)
            assert isinstance(deserialized, Message)
            assert deserialized.text == "Test"
            assert deserialized.sender == "User"
            assert deserialized.sender_name == "User"
            
        except ImportError:
            pytest.skip("Langflow not available for type conversion testing")
    
    @pytest.mark.parametrize("workflow_file", [
        "simple_chat.json",
        "openai_chat.json",
        "basic_prompting.json",
        "memory_chatbot.json",
        "document_qa.json",
        "simple_agent.json"
    ])
    @pytest.mark.asyncio
    async def test_end_to_end_execution_flow(
        self,
        workflow_file: str,
        langflow_fixtures_dir: Path,
        mock_context: MockStepflowContext,
        converter: LangflowConverter,
        udf_executor: UDFExecutor
    ):
        """Test end-to-end execution flow for different workflows."""
        
        workflow_path = langflow_fixtures_dir / workflow_file
        if not workflow_path.exists():
            pytest.skip(f"Workflow file not found: {workflow_file}")
        
        with open(workflow_path, "r") as f:
            langflow_data = json.load(f)
        
        # Convert to Stepflow
        stepflow_workflow = converter.convert(langflow_data)
        
        # Validate workflow structure for execution
        assert len(stepflow_workflow.steps) > 0
        
        for step in stepflow_workflow.steps:
            # Should be either a Langflow component or UDF executor
            assert step.component.startswith("/langflow/")
            
            # Only UDF executors should have blob_id
            if step.component == "/langflow/udf_executor":
                assert "blob_id" in step.input
                # Should have valid blob structure
                # (In real execution, these would be resolved by Stepflow runtime)
                assert isinstance(step.input["blob_id"], str)
    
    async def _create_mock_input_component(self, context: MockStepflowContext) -> str:
        """Create a mock input component."""
        code = '''
from langflow.schema.message import Message

class MockInput:
    def __init__(self):
        self._parameters = {}
    
    def set_attributes(self, params):
        self._parameters = params
    
    async def generate_output(self):
        return Message(
            text=self._parameters.get('input_value', 'default'),
            sender='User',
            sender_name='User'
        )
'''
        
        blob_data = {
            "code": code,
            "component_type": "MockInput",
            "template": {"input_value": {"type": "str", "value": ""}},
            "outputs": [{"name": "output", "method": "generate_output", "types": ["Message"]}],
            "selected_output": "output"
        }
        
        return await context.put_blob(blob_data)
    
    async def _create_mock_processor_component(self, context: MockStepflowContext) -> str:
        """Create a mock processor component.""" 
        code = '''
from langflow.schema.message import Message

class MockProcessor:
    def __init__(self):
        self._parameters = {}
    
    def set_attributes(self, params):
        self._parameters = params
    
    async def process_data(self):
        input_msg = self._parameters.get('input_data', {})
        
        # Extract text from various input formats
        if hasattr(input_msg, 'text'):
            # Real Langflow Message object
            input_text = input_msg.text
        elif isinstance(input_msg, dict) and 'text' in input_msg:
            # Serialized Message dict
            input_text = input_msg['text']
        else:
            # Fallback to string representation
            input_text = str(input_msg)
        
        return Message(
            text=f"PROCESSED: {input_text}",
            sender='AI', 
            sender_name='Processor'
        )
'''
        
        blob_data = {
            "code": code,
            "component_type": "MockProcessor", 
            "template": {"input_data": {"type": "object", "value": {}}},
            "outputs": [{"name": "output", "method": "process_data", "types": ["Message"]}],
            "selected_output": "output"
        }
        
        return await context.put_blob(blob_data)


@pytest.mark.slow
class TestRealLangflowIntegration:
    """Tests that require actual Langflow installation."""
    
    @pytest.mark.asyncio
    async def test_with_real_langflow_types(self):
        """Test with real Langflow types if available."""
        try:
            from langflow.schema.message import Message
            from langflow.schema.data import Data
            
            # Test type converter with real types
            converter = TypeConverter()
            
            # Test Message
            message = Message(text="Test", sender="User")
            serialized = converter.serialize_langflow_object(message)
            assert serialized["__langflow_type__"] == "Message"
            
            deserialized = converter.deserialize_to_langflow_type(serialized)
            assert isinstance(deserialized, Message)
            assert deserialized.text == "Test"
            
        except ImportError:
            pytest.skip("Langflow not available for real type testing")