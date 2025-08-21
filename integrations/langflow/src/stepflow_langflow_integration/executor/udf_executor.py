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

"""UDF executor for Langflow components."""

import os
import sys
import asyncio
import inspect
from typing import Dict, Any, Optional, Type
from stepflow_py import StepflowContext

from .type_converter import TypeConverter
from ..utils.errors import ExecutionError


class UDFExecutor:
    """Executes Langflow components as UDFs with full compatibility."""
    
    def __init__(self):
        """Initialize UDF executor."""
        self.type_converter = TypeConverter()
    
    async def execute(self, input_data: Dict[str, Any], context: StepflowContext) -> Dict[str, Any]:
        """Execute a Langflow component UDF.
        
        Args:
            input_data: Component input containing blob_id and runtime inputs
            context: Stepflow context for blob operations
            
        Returns:
            Component execution result
        """
        # Debug to file since stdout might be captured by Stepflow runtime
        with open('/tmp/udf_debug.log', 'a') as f:
            f.write(f"DEBUG UDF Executor: Starting execution with input keys: {list(input_data.keys())}\n")
        print(f"DEBUG UDF Executor: Starting execution with input keys: {list(input_data.keys())}")
        try:
            # Get UDF blob data
            blob_id = input_data.get("blob_id")
            if not blob_id:
                raise ExecutionError("No blob_id provided")
            
            # Handle blob_id that might be a literal string or already resolved
            if isinstance(blob_id, str):
                # Try to get existing blob, or create special components on-demand
                blob_data = None
                try:
                    blob_data = await context.get_blob(blob_id)
                except Exception as e:
                    # Check if this is a special ChatInput/ChatOutput/File component
                    if "chatinput" in blob_id.lower():
                        blob_data = self._create_chat_input_blob()
                        # Store the blob for future use
                        actual_blob_id = await context.put_blob(blob_data)
                    elif "chatoutput" in blob_id.lower():
                        blob_data = self._create_chat_output_blob()
                        # Store the blob for future use
                        actual_blob_id = await context.put_blob(blob_data)
                    elif "file" in blob_id.lower():
                        blob_data = self._create_file_component_blob()
                        # Store the blob for future use
                        actual_blob_id = await context.put_blob(blob_data)
                    else:
                        # Re-raise the original error for non-special components
                        raise ExecutionError(f"Blob not found: {blob_id}") from e
            else:
                # blob_id should already be resolved by Stepflow from the blob creation step
                raise ExecutionError(f"Invalid blob_id format: {blob_id}")
            
            runtime_inputs = input_data.get("input", {})
            
            # Execute the component
            result = await self._execute_langflow_component(
                blob_data=blob_data,
                runtime_inputs=runtime_inputs,
            )
            
            # Serialize result for Stepflow
            serialized_result = self.type_converter.serialize_langflow_object(result)
            return {"result": serialized_result}
            
        except ExecutionError:
            raise
        except Exception as e:
            raise ExecutionError(f"UDF execution failed: {e}") from e
    
    async def _execute_langflow_component(
        self,
        blob_data: Dict[str, Any], 
        runtime_inputs: Dict[str, Any]
    ) -> Any:
        """Execute a Langflow component with proper class handling.
        
        Args:
            blob_data: UDF blob containing code and metadata
            runtime_inputs: Runtime inputs from other workflow steps
            
        Returns:
            Component execution result
        """
        # Extract UDF components
        code = blob_data.get("code", "")
        template = blob_data.get("template", {})
        component_type = blob_data.get("component_type", "")
        outputs = blob_data.get("outputs", [])
        selected_output = blob_data.get("selected_output")
        
        if not code:
            raise ExecutionError(f"No code found for component {component_type}")
        
        # DEBUG: Log API key availability and component info
        openai_key = os.environ.get("OPENAI_API_KEY", "NOT_SET")
        with open('/tmp/udf_debug.log', 'a') as f:
            f.write(f"DEBUG UDF Executor: Component {component_type}\n")
            f.write(f"DEBUG UDF Executor: OPENAI_API_KEY available: {openai_key[:10] if openai_key != 'NOT_SET' else 'NOT_SET'}...\n")
            f.write(f"DEBUG UDF Executor: Runtime inputs keys: {list(runtime_inputs.keys())}\n")
            f.write(f"DEBUG UDF Executor: Template keys: {list(template.keys())}\n")
            
            # Check for embedded configurations
            embedding_config_keys = [k for k in template.keys() if k.startswith('_embedding_config_')]
            if embedding_config_keys:
                f.write(f"DEBUG UDF Executor: Found embedding config keys: {embedding_config_keys}\n")
            else:
                f.write(f"DEBUG UDF Executor: No embedding config keys found\n")
        print(f"DEBUG UDF Executor: Component {component_type}")
        print(f"DEBUG UDF Executor: OPENAI_API_KEY available: {openai_key[:10] if openai_key != 'NOT_SET' else 'NOT_SET'}...")
        print(f"DEBUG UDF Executor: Runtime inputs keys: {list(runtime_inputs.keys())}")
        print(f"DEBUG UDF Executor: Template keys: {list(template.keys())}")
        
        # Set up execution environment
        exec_globals = self._create_execution_environment()
        
        try:
            # Execute component code
            exec(code, exec_globals)
        except Exception as e:
            print(f"DEBUG UDF Executor: Code execution failed: {e}")
            raise ExecutionError(f"Failed to execute component code: {e}")
        
        # Find component class
        component_class = self._find_component_class(exec_globals, component_type)
        if not component_class:
            raise ExecutionError(f"Component class {component_type} not found")
        
        # Instantiate component
        try:
            component_instance = component_class()
        except Exception as e:
            raise ExecutionError(f"Failed to instantiate {component_type}: {e}")
        
        # Configure component
        print(f"DEBUG UDF Executor: Preparing parameters for {component_type}")
        with open('/tmp/udf_debug.log', 'a') as f:
            f.write(f"DEBUG UDF Executor: Preparing parameters for {component_type}\n")
            
        component_parameters = await self._prepare_component_parameters(
            template, runtime_inputs
        )
        
        with open('/tmp/udf_debug.log', 'a') as f:
            f.write(f"DEBUG UDF Executor: Prepared parameters: {list(component_parameters.keys())}\n")
            # DEBUG: Check for API key in parameters
            for key, value in component_parameters.items():
                if 'api' in key.lower() or 'key' in key.lower():
                    f.write(f"DEBUG UDF Executor: API-related param {key}: {str(value)[:20]}...\n")
                    
        print(f"DEBUG UDF Executor: Prepared parameters: {list(component_parameters.keys())}")
        
        # DEBUG: Check for API key in parameters
        for key, value in component_parameters.items():
            if 'api' in key.lower() or 'key' in key.lower():
                print(f"DEBUG UDF Executor: API-related param {key}: {str(value)[:20]}...")
        
        
        # Use Langflow's configuration method
        with open('/tmp/udf_debug.log', 'a') as f:
            f.write(f"DEBUG UDF Executor: Configuring component {component_type}\n")
            f.write(f"DEBUG UDF Executor: Component has set_attributes: {hasattr(component_instance, 'set_attributes')}\n")
            
        if hasattr(component_instance, "set_attributes"):
            component_instance._parameters = component_parameters
            component_instance.set_attributes(component_parameters)
            with open('/tmp/udf_debug.log', 'a') as f:
                f.write(f"DEBUG UDF Executor: Component configured successfully\n")
        
        # Execute component method
        with open('/tmp/udf_debug.log', 'a') as f:
            f.write(f"DEBUG UDF Executor: Determining execution method for {component_type}\n")
            f.write(f"DEBUG UDF Executor: Outputs: {outputs}\n")
            f.write(f"DEBUG UDF Executor: Selected output: {selected_output}\n")
            
        execution_method = self._determine_execution_method(outputs, selected_output)
        
        # If no method found from metadata, try to infer from the component class
        with open('/tmp/udf_debug.log', 'a') as f:
            f.write(f"DEBUG UDF Executor: Execution method from metadata: {execution_method}\n")
            
        if not execution_method:
            execution_method = self._infer_execution_method_from_component(component_instance)
            with open('/tmp/udf_debug.log', 'a') as f:
                f.write(f"DEBUG UDF Executor: Inferred execution method: {execution_method}\n")
        
        if not execution_method:
            available = [m for m in dir(component_instance) if not m.startswith("_")]
            with open('/tmp/udf_debug.log', 'a') as f:
                f.write(f"DEBUG UDF Executor: No execution method found. Available: {available}\n")
            raise ExecutionError(
                f"No execution method found for {component_type}. "
                f"Available methods: {available}"
            )
        
        with open('/tmp/udf_debug.log', 'a') as f:
            f.write(f"DEBUG UDF Executor: Final execution method: {execution_method}\n")
            f.write(f"DEBUG UDF Executor: Component has method {execution_method}: {hasattr(component_instance, execution_method)}\n")
            
        if not hasattr(component_instance, execution_method):
            available = [m for m in dir(component_instance) if not m.startswith("_")]
            with open('/tmp/udf_debug.log', 'a') as f:
                f.write(f"DEBUG UDF Executor: Method {execution_method} not found. Available: {available}\n")
            raise ExecutionError(
                f"Method {execution_method} not found in {component_type}. "
                f"Available: {available}"
            )
        
        try:
            method = getattr(component_instance, execution_method)
            
            with open('/tmp/udf_debug.log', 'a') as f:
                f.write(f"DEBUG UDF Executor: About to execute method {execution_method} for {component_type}\n")
            
            if inspect.iscoroutinefunction(method):
                result = await method()
            else:
                # Handle sync methods safely
                result = await self._execute_sync_method_safely(method, component_type)
            
            with open('/tmp/udf_debug.log', 'a') as f:
                f.write(f"DEBUG UDF Executor: Execution successful for {component_type}, result type: {type(result)}\n")
            
            return result
            
        except Exception as e:
            with open('/tmp/udf_debug.log', 'a') as f:
                f.write(f"DEBUG UDF Executor: EXECUTION FAILED for {component_type}: {e}\n")
                import traceback
                f.write(f"DEBUG UDF Executor: Full traceback: {traceback.format_exc()}\n")
            raise ExecutionError(f"Failed to execute {execution_method}: {e}")
    
    def _create_execution_environment(self) -> Dict[str, Any]:
        """Create safe execution environment with Langflow imports."""
        exec_globals = globals().copy()
        exec_globals["os"] = os
        exec_globals["sys"] = sys
        
        # Import common Langflow types
        try:
            print("DEBUG UDF Executor: Attempting Langflow imports...")
            from langflow.schema.message import Message
            from langflow.schema.data import Data
            from langflow.schema.dataframe import DataFrame
            from langflow.custom.custom_component.component import Component
            
            exec_globals.update({
                "Message": Message,
                "Data": Data,
                "DataFrame": DataFrame,
                "Component": Component,
            })
            print("DEBUG UDF Executor: Langflow imports successful")
        except ImportError as e:
            print(f"DEBUG UDF Executor: Langflow import failed: {e}")
            raise ExecutionError(f"Failed to import Langflow components: {e}")
        
        return exec_globals
    
    def _find_component_class(
        self, 
        exec_globals: Dict[str, Any], 
        component_type: str
    ) -> Optional[Type]:
        """Find component class in execution environment."""
        component_class = exec_globals.get(component_type)
        if component_class and isinstance(component_class, type):
            return component_class
        
        # Search for class by name with different matching strategies
        component_type_lower = component_type.lower()
        
        for name, obj in exec_globals.items():
            if not isinstance(obj, type):
                continue
                
            name_lower = name.lower()
            
            # Strategy 1: Exact lowercase match
            if name_lower == component_type_lower:
                return obj
            
            # Strategy 2: Component suffix match (e.g., "Prompt" -> "PromptComponent")
            if name_lower == component_type_lower + "component":
                return obj
            
            # Strategy 3: Component prefix match (e.g., "PromptComponent" -> "Prompt") 
            if name_lower.endswith("component") and name_lower[:-9] == component_type_lower:
                return obj
        
        return None
    
    async def _prepare_component_parameters(
        self, 
        template: Dict[str, Any], 
        runtime_inputs: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Prepare component parameters from template and runtime inputs."""
        component_parameters = {}
        
        # Process template parameters
        for key, field_def in template.items():
            if isinstance(field_def, dict) and "value" in field_def:
                value = field_def["value"]
                
                # Handle environment variables
                env_var = self._determine_environment_variable(key, value, field_def)
                if env_var:
                    actual_value = os.getenv(env_var)
                    if actual_value:
                        value = actual_value
                    elif field_def.get("_input_type") == "SecretStrInput":
                        print(f"⚠️  Environment variable {env_var} not found for {key}", file=sys.stderr)
                
                component_parameters[key] = value
            
            # Handle complex configuration objects (embedded components)
            elif key.startswith("_embedding_config_"):
                # This is an embedded OpenAI Embeddings configuration
                embedding_field = key.replace("_embedding_config_", "")
                embedding_config = field_def.get("value", {})
                
                if embedding_config.get("component_type") == "OpenAIEmbeddings":
                    # Instantiate the OpenAI Embeddings component
                    try:
                        from langchain_openai import OpenAIEmbeddings
                        embedding_params = embedding_config.get("config", {})
                        
                        # Handle API key from environment
                        if "api_key" in embedding_params:
                            api_key_value = embedding_params["api_key"]
                            if isinstance(api_key_value, str) and api_key_value.startswith("${") and api_key_value.endswith("}"):
                                env_var = api_key_value[2:-1]
                                actual_api_key = os.getenv(env_var)
                                if actual_api_key:
                                    embedding_params["api_key"] = actual_api_key
                        
                        # Create embeddings instance
                        embeddings = OpenAIEmbeddings(**embedding_params)
                        component_parameters[embedding_field] = embeddings
                        
                        print(f"DEBUG UDF Executor: Created embedded OpenAI Embeddings for {embedding_field}")
                        with open('/tmp/udf_debug.log', 'a') as f:
                            f.write(f"DEBUG UDF Executor: Created embedded OpenAI Embeddings for {embedding_field}\n")
                    except Exception as e:
                        print(f"Warning: Failed to create embedded OpenAI Embeddings: {e}")
                        with open('/tmp/udf_debug.log', 'a') as f:
                            f.write(f"Warning: Failed to create embedded OpenAI Embeddings: {e}\n")
        
        # Add runtime inputs (these override template values)
        for key, value in runtime_inputs.items():
            # Convert Stepflow values back to Langflow types if needed
            if isinstance(value, dict) and "__langflow_type__" in value:
                actual_value = self.type_converter.deserialize_to_langflow_type(value)
                component_parameters[key] = actual_value
            else:
                component_parameters[key] = value
        
        return component_parameters
    
    def _determine_environment_variable(
        self, 
        field_name: str, 
        field_value: Any, 
        field_config: Dict[str, Any]
    ) -> Optional[str]:
        """Determine environment variable name for a field."""
        # Template string like "${OPENAI_API_KEY}"
        if (isinstance(field_value, str) and 
            field_value.startswith("${") and field_value.endswith("}")):
            return field_value[2:-1]
        
        # Direct env var name
        if (isinstance(field_value, str) and 
            field_value.isupper() and 
            "_" in field_value and 
            any(keyword in field_value for keyword in ["API_KEY", "TOKEN", "SECRET"])):
            return field_value
        
        # Secret input fields
        if field_config.get("_input_type") == "SecretStrInput":
            if field_name == "api_key":
                return "OPENAI_API_KEY"
            elif "openai" in field_name.lower():
                return "OPENAI_API_KEY"
            elif "anthropic" in field_name.lower():
                return "ANTHROPIC_API_KEY"
            else:
                return field_name.upper()
        
        return None
    
    def _determine_execution_method(
        self, 
        outputs: list, 
        selected_output: Optional[str]
    ) -> Optional[str]:
        """Determine execution method from outputs metadata."""
        if selected_output:
            for output in outputs:
                if output.get("name") == selected_output:
                    method = output.get("method")
                    if method:
                        return method
        
        # Fallback to first output's method
        if outputs:
            method = outputs[0].get("method")
            if method:
                return method
        
        # Final fallback: try common method names for components without metadata
        # This handles cases where outputs metadata is missing but the component has standard methods
        return None
    
    def _infer_execution_method_from_component(self, component_instance) -> Optional[str]:
        """Infer execution method by examining the component class definition.
        
        This handles cases where outputs metadata is missing but the component 
        has standard methods that we can detect.
        """
        # Check if the component has an outputs attribute we can examine
        if hasattr(component_instance, 'outputs'):
            outputs = getattr(component_instance, 'outputs', [])
            if outputs and hasattr(outputs[0], 'method'):
                return outputs[0].method
        
        # Look for common Langflow component method patterns
        common_methods = [
            'build_prompt',      # PromptComponent
            'process_message',   # ChatInput/ChatOutput
            'build_model',       # Model components  
            'execute',           # Generic execute method
            'process',           # Generic process method
            'build',             # Generic build method
            # Removed: 'embed_documents' - embeddings are now complex configuration objects
            'embed_query',       # OpenAI Embeddings query method (kept for query operations)
            # Removed: 'aembed_documents' - embeddings are now complex configuration objects
            'run',              # Standard component run method
            'invoke',           # LangChain invoke method
        ]
        
        for method_name in common_methods:
            if hasattr(component_instance, method_name):
                return method_name
        
        # If component has __call__, use that
        if hasattr(component_instance, '__call__'):
            return '__call__'
        
        return None
    
    async def _execute_sync_method_safely(self, method, component_type: str):
        """Execute sync method safely in async context."""
        # For problematic components, use thread pool
        if component_type in ["URLComponent", "RecursiveUrlLoader"]:
            import concurrent.futures
            loop = asyncio.get_event_loop()
            with concurrent.futures.ThreadPoolExecutor() as executor:
                future = executor.submit(method)
                return await loop.run_in_executor(None, lambda: future.result())
        else:
            return method()
    
    def _create_chat_input_blob(self) -> Dict[str, Any]:
        """Create blob data for ChatInput component."""
        chat_input_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import MessageTextInput, Output
from langflow.schema.message import Message

class ChatInputComponent(Component):
    display_name = "Chat Input"
    description = "Processes chat input from workflow"
    
    inputs = [
        MessageTextInput(name="message", display_name="Message Input"),
    ]
    
    outputs = [
        Output(display_name="Output", name="output", method="process_message")
    ]

    def process_message(self) -> Message:
        """Process the input message."""
        message_text = self.message or "No message provided"
        return Message(
            text=str(message_text),
            sender="User",
            sender_name="User"
        )
'''
        
        return {
            "code": chat_input_code,
            "component_type": "ChatInputComponent",
            "template": {
                "message": {
                    "type": "str", 
                    "value": "",
                    "info": "Message text input"
                }
            },
            "outputs": [{"name": "output", "method": "process_message", "types": ["Message"]}],
            "selected_output": "output"
        }
    
    def _create_chat_output_blob(self) -> Dict[str, Any]:
        """Create blob data for ChatOutput component."""
        chat_output_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import HandleInput, Output
from langflow.schema.message import Message

class ChatOutputComponent(Component):
    display_name = "Chat Output"
    description = "Processes chat output for workflow"
    
    inputs = [
        HandleInput(name="input_message", display_name="Input Message", input_types=["Message", "str"])
    ]
    
    outputs = [
        Output(display_name="Output", name="output", method="process_output")
    ]

    def process_output(self) -> Message:
        """Process the output message."""
        input_msg = self.input_message
        
        # Handle different input types
        if hasattr(input_msg, 'text'):
            # It's already a Message object
            return input_msg
        elif isinstance(input_msg, dict):
            # It's a dict with message fields
            return Message(
                text=input_msg.get('text', str(input_msg)),
                sender=input_msg.get('sender', 'AI'),
                sender_name=input_msg.get('sender_name', 'Assistant')
            )
        else:
            # Convert to string and create Message
            return Message(
                text=str(input_msg),
                sender="AI",
                sender_name="Assistant"
            )
'''
        
        return {
            "code": chat_output_code,
            "component_type": "ChatOutputComponent", 
            "template": {
                "input_message": {
                    "type": "Message",
                    "value": None,
                    "info": "Input message to output"
                }
            },
            "outputs": [{"name": "output", "method": "process_output", "types": ["Message"]}],
            "selected_output": "output"
        }
    
    def _create_file_component_blob(self) -> Dict[str, Any]:
        """Create blob data for File component with mock content."""
        file_component_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.message import Message

class MockFileComponent(Component):
    display_name = "Mock File"
    description = "Mock file component that provides sample content for testing"
    
    outputs = [
        Output(display_name="Raw Content", name="message", method="load_files_message")
    ]

    def load_files_message(self) -> Message:
        """Return mock file content."""
        mock_content = """Sample Document Content
        
This is a mock document that serves as sample content for testing the document Q&A workflow.

Key information:
- This document discusses various topics related to AI and machine learning
- It contains technical concepts and explanations
- The content is suitable for question-answering tasks
- You can ask questions about AI, machine learning, or general topics

Technical Details:
- Machine learning is a subset of artificial intelligence
- Deep learning uses neural networks with multiple layers
- Natural language processing helps computers understand human language
- Large language models are trained on vast amounts of text data

This mock content allows testing of the document processing pipeline without requiring actual file uploads."""

        return Message(
            text=mock_content,
            sender="System",
            sender_name="File Component"
        )
'''
        
        return {
            "code": file_component_code,
            "component_type": "MockFileComponent",
            "template": {
                "path": {
                    "type": "file",
                    "value": "",
                    "info": "Mock file path (not used in testing)"
                }
            },
            "outputs": [{"name": "message", "method": "load_files_message", "types": ["Message"]}],
            "selected_output": "message"
        }