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

"""Process individual Langflow nodes into Stepflow steps."""

import json
from typing import Dict, Any, List, Optional

from stepflow_py import FlowBuilder, Value

from ..types.stepflow import StepflowStep 
from ..utils.errors import ConversionError
from .schema_mapper import SchemaMapper


class NodeProcessor:
    """Processes individual Langflow nodes into Stepflow steps."""
    
    def __init__(self):
        """Initialize node processor."""
        self.schema_mapper = SchemaMapper()
    
    def process_node(
        self, 
        node: Dict[str, Any], 
        dependencies: Dict[str, List[str]],
        all_nodes: List[Dict[str, Any]],
        builder: FlowBuilder,
        node_output_refs: Dict[str, Any],
        field_mapping: Dict[str, Dict[str, str]] = None
    ) -> Optional[Any]:
        """Process a Langflow node using flow builder architecture.
        
        Args:
            node: Langflow node object
            dependencies: Dependency graph for all nodes
            all_nodes: All nodes in the workflow
            builder: FlowBuilder instance
            node_output_refs: Mapping of node IDs to their output references
            field_mapping: Mapping of target nodes to their input field names from edges
            
        Returns:
            Output reference for this node, or None if node should be skipped
        """
        try:
            node_id = node.get("id")
            if not node_id:
                raise ConversionError("Node missing ID")
            
            # Check if this is a note or documentation node that should be skipped
            node_type = node.get("type", "")
            if node_type == "noteNode":
                # This is a React Flow note node - skip it entirely
                return None
            
            node_data = node.get("data", {})
            component_type = node_data.get("type", "")
            
            # Skip nodes without a valid component type (documentation nodes, etc.)
            if not component_type or component_type.strip() == "":
                return None
            
            # Generate step ID (clean up for Stepflow)
            step_id = self._generate_step_id(node_id, component_type)
            
            # Handle special components first
            if component_type == "ChatInput":
                # ChatInput returns a reference to workflow input directly
                return Value.input.add_path("message")
            elif component_type == "ChatOutput":
                # ChatOutput depends on another node - return that node's output reference
                dependency_node_ids = dependencies.get(node_id, [])
                if dependency_node_ids and dependency_node_ids[0] in node_output_refs:
                    return node_output_refs[dependency_node_ids[0]]
                else:
                    # ChatOutput with no dependencies - return input passthrough
                    return Value.input.add_path("message")
            
            # For all other components, create a step and return a reference to it
            node_info = node_data.get("node", {})
            template = node_info.get("template", {})
            custom_code = template.get("code", {}).get("value", "")
            
            # Check for embedded configuration marker
            has_embedded_config = template.get("_has_embedded_config", {}).get("value", False)
            
            # Determine component path and inputs
            if component_type == "File":
                component_path = "/langflow/file"
                step_input = self._extract_component_inputs_for_builder(node, dependencies.get(node_id, []), all_nodes, node_output_refs)
            elif component_type == "Memory":
                component_path = "/langflow/memory" 
                step_input = self._extract_component_inputs_for_builder(node, dependencies.get(node_id, []), all_nodes, node_output_refs)
            elif component_type == "URLComponent":
                component_path = "/langflow/url"
                step_input = self._extract_component_inputs_for_builder(node, dependencies.get(node_id, []), all_nodes, node_output_refs)
            elif has_embedded_config and component_type in ["AstraDB", "VectorStore", "Chroma", "FAISS", "Pinecone"]:
                # Vector store with embedded configuration - use standalone server
                print(f"DEBUG NodeProcessor: Routing {component_type} with embedded config to standalone server")
                component_path = f"/langflow/{component_type}"
                step_input = self._extract_component_inputs_for_builder(node, dependencies.get(node_id, []), all_nodes, node_output_refs)
            elif custom_code:
                # Custom component - use UDF executor
                component_path = "/langflow/udf_executor"
                
                # First create a blob step for the UDF code using auto ID generation
                blob_data = self._prepare_udf_blob(node, component_type)
                
                blob_step_handle = builder.add_step_auto(
                    base_name="blob",
                    component="/builtin/put_blob",
                    input_data={
                        "data": blob_data,
                        "blob_type": "data"
                    }
                )
                
                # Now create the UDF executor step that uses the blob
                step_input = {
                    "blob_id": Value.step(blob_step_handle.id, "blob_id"),
                    "input": self._extract_runtime_inputs_for_builder(node, dependencies.get(node_id, []), node_output_refs, field_mapping)
                }
            else:
                # Built-in component - use Langflow component directly
                component_path = f"/langflow/{component_type}"
                step_input = self._extract_component_inputs_for_builder(node, dependencies.get(node_id, []), all_nodes, node_output_refs)
            
            # Add step to builder using auto ID generation and data conversion
            step_handle = builder.add_step_auto(
                base_name=component_type.lower(),
                component=component_path,
                input_data=step_input
            )
            
            # Return a reference to this step's output
            return Value.step(step_handle.id, "result")
            
        except Exception as e:
            import traceback
            print(f"Full traceback for node {node.get('id', 'unknown')}: {traceback.format_exc()}")
            raise ConversionError(f"Error processing node {node.get('id', 'unknown')}: {e}")
    
    def _generate_step_id(self, node_id: str, component_type: str) -> str:
        """Generate a clean step ID from node ID and type.
        
        Args:
            node_id: Original Langflow node ID
            component_type: Component type name
            
        Returns:
            Clean step ID suitable for Stepflow
        """
        # Always use the full node_id to ensure uniqueness
        # For any node_id with a suffix, keep it to ensure uniqueness
        base_id = node_id.lower()
        
        # Always use langflow prefix with the full base_id to guarantee uniqueness
        return f"langflow_{base_id}"
    
    def _prepare_udf_blob(self, node: Dict[str, Any], component_type: str) -> Dict[str, Any]:
        """Prepare UDF blob data for component execution.
        
        Args:
            node: Langflow node object
            component_type: Component type name
            
        Returns:
            UDF blob data
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        
        # Extract component code
        template = node_info.get("template", {})
        code = template.get("code", {}).get("value", "")
        
        if not code:
            raise ConversionError(f"No code found for component {component_type}")
        
        # Extract outputs information
        outputs = node_data.get("outputs", [])
        selected_output = None
        if outputs:
            # For now, use the first output
            selected_output = outputs[0].get("name")
        
        # Prepare template (remove code field to avoid duplication)
        prepared_template = {}
        embedding_config_count = 0
        for field_name, field_config in template.items():
            if field_name != "code":
                prepared_template[field_name] = field_config
                if field_name.startswith("_embedding_config_"):
                    embedding_config_count += 1
        
        # Debug: Log embedded configuration preservation
        if embedding_config_count > 0:
            print(f"DEBUG NodeProcessor: Preserved {embedding_config_count} embedding configs for {component_type}")
        else:
            print(f"DEBUG NodeProcessor: No embedding configs found for {component_type}")
        
        return {
            "code": code,
            "template": prepared_template,
            "component_type": component_type,
            "outputs": outputs,
            "selected_output": selected_output,
        }
    
    def _extract_runtime_inputs(
        self, 
        node: Dict[str, Any], 
        dependency_node_ids: List[str]
    ) -> Dict[str, Any]:
        """Extract runtime inputs that will come from other workflow steps.
        
        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on
            
        Returns:
            Dict of runtime inputs
        """
        runtime_inputs = {}
        
        # For now, create placeholder inputs based on dependencies
        # TODO: Map actual edge connections to specific input fields
        for i, dep_id in enumerate(dependency_node_ids):
            dep_step_id = self._generate_step_id(dep_id, "")
            runtime_inputs[f"input_{i}"] = {
                "$from": {"step": dep_step_id},
                "path": "result"
            }
        
        return runtime_inputs
    
    def _extract_component_inputs_for_builder(
        self, 
        node: Dict[str, Any], 
        dependency_node_ids: List[str],
        all_nodes: List[Dict[str, Any]],
        node_output_refs: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Extract inputs for built-in Langflow components using flow builder architecture.
        
        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on
            all_nodes: All nodes in the workflow
            node_output_refs: Mapping of node IDs to their output references
            
        Returns:
            Dict of component inputs mapped to template fields
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        template = node_info.get("template", {})
        
        component_inputs = {}
        
        # Map static template values to component inputs
        for field_name, field_config in template.items():
            if isinstance(field_config, dict):
                field_value = field_config.get("value")
                if field_value is not None:
                    component_inputs[field_name] = field_value
        
        # Map dependencies to component inputs based on edges
        if dependency_node_ids:
            dep_node_id = dependency_node_ids[0]
            
            # Check if we have an output reference for this dependency
            if dep_node_id in node_output_refs:
                input_reference = node_output_refs[dep_node_id]
            else:
                # Find the dependency node to check its type
                dep_node = None
                for n in all_nodes:
                    if n.get("id") == dep_node_id:
                        dep_node = n
                        break
                
                # Check if dependency is a ChatInput component
                if dep_node and dep_node.get("data", {}).get("type") == "ChatInput":
                    # Reference workflow input instead of missing ChatInput step
                    input_reference = Value.input("$.message")
                else:
                    # This shouldn't happen with the new architecture, but fallback
                    input_reference = Value.input("$.message")
            
            # Common input field names for different component types
            component_type = node_data.get("type", "")
            if component_type in ["ChatOutput", "TextOutput"]:
                component_inputs["input_value"] = input_reference
            elif component_type in ["LanguageModelComponent", "OpenAIModelComponent"]:
                component_inputs["input_value"] = input_reference
            else:
                # Generic mapping
                component_inputs["input"] = input_reference
        
        return component_inputs
    
    def _extract_runtime_inputs_for_builder(
        self, 
        node: Dict[str, Any], 
        dependency_node_ids: List[str],
        node_output_refs: Dict[str, Any],
        field_mapping: Dict[str, Dict[str, str]] = None
    ) -> Dict[str, Any]:
        """Extract runtime inputs for UDF components using flow builder architecture.
        
        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on
            node_output_refs: Mapping of node IDs to their output references
            field_mapping: Mapping of target nodes to their input field names from edges
            
        Returns:
            Dict of runtime inputs
        """
        runtime_inputs = {}
        node_id = node.get("id")
        
        # Use field mapping if available, otherwise fall back to generic input names
        if field_mapping and node_id in field_mapping:
            # Map dependencies to their proper field names
            node_field_map = field_mapping[node_id]
            
            # Group inputs by field name to handle list fields like 'tools'
            field_inputs = {}
            
            for dep_id in dependency_node_ids:
                if dep_id in node_field_map and dep_id in node_output_refs:
                    field_name = node_field_map[dep_id]
                    
                    # Handle list fields by collecting multiple inputs
                    if field_name not in field_inputs:
                        field_inputs[field_name] = []
                    field_inputs[field_name].append(node_output_refs[dep_id])
                elif dep_id in node_output_refs:
                    # Fallback to generic name if no field mapping
                    runtime_inputs[f"input_{len(runtime_inputs)}"] = node_output_refs[dep_id]
            
            # Convert to runtime inputs format
            for field_name, inputs in field_inputs.items():
                if len(inputs) == 1:
                    # Single input - use directly
                    runtime_inputs[field_name] = inputs[0]
                else:
                    # Multiple inputs - create a list
                    runtime_inputs[field_name] = inputs
        else:
            # Fallback to old behavior for backwards compatibility
            for i, dep_id in enumerate(dependency_node_ids):
                if dep_id in node_output_refs:
                    runtime_inputs[f"input_{i}"] = node_output_refs[dep_id]
                else:
                    # Fallback to workflow input if dependency not found
                    runtime_inputs[f"input_{i}"] = Value.input("$.message")
        
        return runtime_inputs
    
    def _extract_component_inputs(
        self, 
        node: Dict[str, Any], 
        dependency_node_ids: List[str],
        all_nodes: List[Dict[str, Any]] = None
    ) -> Dict[str, Any]:
        """Extract inputs for built-in Langflow components.
        
        Args:
            node: Langflow node object
            dependency_node_ids: IDs of nodes this node depends on
            
        Returns:
            Dict of component inputs mapped to template fields
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        template = node_info.get("template", {})
        
        component_inputs = {}
        
        # Map static template values to component inputs
        for field_name, field_config in template.items():
            if isinstance(field_config, dict):
                field_value = field_config.get("value")
                if field_value is not None:
                    component_inputs[field_name] = field_value
        
        # Map dependencies to component inputs based on edges
        if dependency_node_ids and all_nodes:
            # Find the dependency node to check its type
            dep_node_id = dependency_node_ids[0]
            dep_node = None
            for n in all_nodes:
                if n.get("id") == dep_node_id:
                    dep_node = n
                    break
            
            # Check if dependency is a ChatInput component
            if dep_node and dep_node.get("data", {}).get("type") == "ChatInput":
                # Reference workflow input instead of missing ChatInput step
                input_reference = {
                    "$from": {"workflow": "input"},
                    "path": "message"
                }
            else:
                # Reference the dependency step normally
                dep_step_id = self._generate_step_id(dep_node_id, "")
                input_reference = {
                    "$from": {"step": dep_step_id},
                    "path": "result"
                }
            
            # Common input field names for different component types
            component_type = node_data.get("type", "")
            if component_type in ["ChatOutput", "TextOutput"]:
                component_inputs["input_value"] = input_reference
            elif component_type in ["LanguageModelComponent", "OpenAIModelComponent"]:
                component_inputs["input_value"] = input_reference
            else:
                # Generic mapping
                component_inputs["input"] = input_reference
        
        return component_inputs
    
    def _extract_chat_input_mapping(self, node: Dict[str, Any]) -> Dict[str, Any]:
        """Extract inputs for ChatInput component to map to workflow input.
        
        Args:
            node: Langflow ChatInput node object
            
        Returns:
            Dict mapping ChatInput to workflow input
        """
        node_data = node.get("data", {})
        node_info = node_data.get("node", {})
        template = node_info.get("template", {})
        
        # Get the sender from template, defaulting to "User" 
        sender = "User"
        if "sender" in template:
            sender_config = template["sender"]
            if isinstance(sender_config, dict):
                sender = sender_config.get("value", "User")
        
        # Create a message from workflow input
        return {
            "message": {
                "$from": {"workflow": "input"},
                "path": "message"  # Expect workflow input to have a 'message' field
            },
            "sender": sender
        }
    
    def _extract_chat_output_mapping(
        self, 
        node: Dict[str, Any], 
        dependency_node_ids: List[str]
    ) -> Dict[str, Any]:
        """Extract inputs for ChatOutput component to pass through data.
        
        Args:
            node: Langflow ChatOutput node object
            dependency_node_ids: IDs of nodes this node depends on
            
        Returns:
            Dict mapping ChatOutput inputs for identity pass-through
        """
        if dependency_node_ids:
            # Pass through the input from the previous step
            dep_step_id = self._generate_step_id(dependency_node_ids[0], "")
            return {
                "input_message": {
                    "$from": {"step": dep_step_id},
                    "path": "result"
                }
            }
        else:
            # No dependencies, return empty value
            return {"input_message": None}
    
    def _create_chat_input_udf(self, node: Dict[str, Any]) -> Dict[str, Any]:
        """Create UDF blob data for ChatInput component.
        
        Args:
            node: Langflow ChatInput node object
            
        Returns:
            UDF blob data for ChatInput handling
        """
        # Create a simple ChatInput UDF that reads from workflow input
        chat_input_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import MessageTextInput, Output
from langflow.schema.message import Message

class ChatInputComponent(Component):
    display_name = "Chat Input"
    description = "Processes chat input from workflow"
    
    inputs = [
        MessageTextInput(name="message", display_name="Message"),
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
    
    def _create_chat_output_udf(self, node: Dict[str, Any]) -> Dict[str, Any]:
        """Create UDF blob data for ChatOutput component.
        
        Args:
            node: Langflow ChatOutput node object
            
        Returns:
            UDF blob data for ChatOutput handling
        """
        # Create a simple ChatOutput UDF that passes through its input
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
    
    def _create_file_component_udf(self, node: Dict[str, Any]) -> Dict[str, Any]:
        """Create UDF blob data for File component with mock content.
        
        Args:
            node: Langflow File node object
            
        Returns:
            UDF blob data for File component handling with mock content
        """
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
    
    def _create_memory_component_udf(self, node: Dict[str, Any]) -> Dict[str, Any]:
        """Create UDF blob data for Memory component with mock memory.
        
        Args:
            node: Langflow Memory node object
            
        Returns:
            UDF blob data for Memory component handling with mock memory
        """
        memory_component_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.message import Message
from typing import List

class MockMemoryComponent(Component):
    display_name = "Mock Memory"
    description = "Mock memory component that provides sample conversation history"
    
    outputs = [
        Output(display_name="Messages Text", name="messages_text", method="get_messages_text")
    ]

    def get_messages_text(self) -> str:
        """Return mock conversation history."""
        mock_history = """Previous conversation history:

User: Hello, how are you?
Assistant: Hello! I'm doing well, thank you for asking. I'm here to help with any questions or tasks you have.

User: What can you help me with?
Assistant: I can help with a wide variety of tasks including answering questions, writing content, analysis, problem-solving, and general conversation. What would you like assistance with today?

User: Tell me about artificial intelligence.
Assistant: Artificial intelligence (AI) refers to computer systems that can perform tasks that typically require human intelligence, such as learning, reasoning, and problem-solving. AI encompasses various techniques like machine learning, deep learning, and natural language processing.
"""
        return mock_history.strip()
'''
        
        return {
            "code": memory_component_code,
            "component_type": "MockMemoryComponent",
            "template": {
                "messages": {
                    "type": "list",
                    "value": [],
                    "info": "Mock message history (not used in testing)"
                }
            },
            "outputs": [{"name": "messages_text", "method": "get_messages_text", "types": ["str"]}],
            "selected_output": "messages_text"
        }
    
    def _create_url_component_udf(self, node: Dict[str, Any]) -> Dict[str, Any]:
        """Create UDF blob data for URL component with mock web content.
        
        Args:
            node: Langflow URL node object
            
        Returns:
            UDF blob data for URL component handling with mock web content
        """
        url_component_code = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.data import Data
from typing import List

class MockURLComponent(Component):
    display_name = "Mock URL"
    description = "Mock URL component that provides sample web content"
    
    outputs = [
        Output(display_name="Data", name="data", method="fetch_content")
    ]

    def fetch_content(self) -> List[Data]:
        """Return mock web content as Data objects."""
        mock_content_1 = """Mock Web Page 1: Mathematics Calculator

This is a mock web page about mathematical calculations and tools.

Content:
- Online calculator for basic arithmetic operations
- Addition, subtraction, multiplication, and division
- Scientific calculator functions
- Step-by-step solution explanations
- Educational math resources

Example: 2 + 2 = 4
The sum of 2 and 2 equals 4. This is a fundamental arithmetic operation."""

        mock_content_2 = """Mock Web Page 2: Mathematical Tools and Resources

This page contains various mathematical tools and educational content.

Features:
- Interactive problem solver
- Math formula reference
- Practice problems with solutions
- Tutorial videos and explanations
- Advanced mathematical concepts

For simple addition like 2 + 2, the answer is always 4."""

        # Create Data objects with the mock content
        data_objects = [
            Data(
                text=mock_content_1,
                data={
                    "url": "https://example.com/calculator",
                    "title": "Mock Calculator Page",
                    "content_type": "text/html"
                }
            ),
            Data(
                text=mock_content_2,
                data={
                    "url": "https://example.com/math-tools", 
                    "title": "Mock Math Tools Page",
                    "content_type": "text/html"
                }
            )
        ]
        
        return data_objects
'''
        
        return {
            "code": url_component_code,
            "component_type": "MockURLComponent",
            "template": {
                "url": {
                    "type": "str",
                    "value": "",
                    "info": "Mock URL (not used in testing)"
                }
            },
            "outputs": [{"name": "data", "method": "fetch_content", "types": ["Data"]}],
            "selected_output": "data"
        }
    
    def _create_simple_mock_component_unused(self, component_name: str, output_type: str) -> Dict[str, Any]:
        """Create a simple mock component that returns appropriate mock data.
        
        Args:
            component_name: Name of the component (File, Memory, URLComponent, etc.)
            output_type: Expected output type (Message, str, Data, etc.)
            
        Returns:
            UDF blob data for the mock component
        """
        # Generate appropriate mock content based on component type
        if component_name == "File":
            mock_content = """Sample Document Content
        
This is a mock document for testing. It contains information about:
- AI and machine learning concepts
- Technical documentation
- Sample data for Q&A workflows

This mock file allows testing without requiring actual file uploads."""
            
            code_template = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output
from langflow.schema.message import Message

class MockFileComponent(Component):
    display_name = "Mock File"
    description = "Mock file component for testing"
    
    outputs = [Output(display_name="Content", name="message", method="get_content")]

    def get_content(self) -> Message:
        return Message(text="""''' + mock_content + '''""", sender="System", sender_name="File Component")
'''
            output_info = {"name": "message", "method": "get_content", "types": ["Message"]}
            
        elif component_name == "Memory":
            mock_content = """Previous conversation:
User: Hello, how are you?
Assistant: I'm doing well, thank you! How can I help you today?
User: What did we discuss before?
Assistant: We've had a brief greeting exchange where you asked how I was doing."""
            
            code_template = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output

class MockMemoryComponent(Component):
    display_name = "Mock Memory"
    description = "Mock memory component for testing"
    
    outputs = [Output(display_name="Messages Text", name="messages_text", method="get_content")]

    def get_content(self) -> str:
        return """''' + mock_content + '''"""
'''
            output_info = {"name": "messages_text", "method": "get_content", "types": ["str"]}
            
        elif component_name == "URLComponent":
            mock_content = """Mock Web Content: Mathematics and Calculation Tools

This is sample web content about mathematical calculations:
- Basic arithmetic operations
- Online calculators and tools  
- Educational math resources
- Problem-solving guides

Example: 2 + 2 = 4 (basic addition)"""
            
            code_template = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output

class MockURLComponentComponent(Component):
    display_name = "Mock URL"
    description = "Mock URL component for testing"
    
    outputs = [Output(display_name="Content", name="content", method="get_content")]

    def get_content(self) -> str:
        return """''' + mock_content + '''"""
'''
            output_info = {"name": "content", "method": "get_content", "types": ["str"]}
            
        else:
            # Generic mock component
            code_template = '''
from langflow.custom.custom_component.component import Component
from langflow.io import Output

class MockGenericComponent(Component):
    display_name = "Mock Generic"
    description = "Mock generic component for testing"
    
    outputs = [Output(display_name="Result", name="result", method="get_content")]

    def get_content(self):
        return "Mock generic result for testing"
'''
            output_info = {"name": "result", "method": "get_content", "types": ["str"]}
        
        return {
            "code": code_template.strip(),
            "component_type": f"Mock{component_name}Component",
            "template": {},
            "outputs": [output_info],
            "selected_output": output_info["name"]
        }