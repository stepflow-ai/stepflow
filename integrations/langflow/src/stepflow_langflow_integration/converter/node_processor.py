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
        dependencies: Dict[str, List[str]]
    ) -> Optional[StepflowStep]:
        """Process a Langflow node into a Stepflow step.
        
        Args:
            node: Langflow node object
            dependencies: Dependency graph for all nodes
            
        Returns:
            StepflowStep object or None if node should be skipped
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
            
            # Check if this is a custom component (has code) or built-in
            node_data = node.get("data", {})
            node_info = node_data.get("node", {})
            template = node_info.get("template", {})
            custom_code = template.get("code", {}).get("value", "")
            
            # Handle special components first
            if component_type == "ChatInput":
                # ChatInput should read from workflow input - create special UDF
                blob_data = self._create_chat_input_udf(node)
                step_input = {
                    "blob_id": f"udf_{step_id}",
                    "input": self._extract_chat_input_mapping(node)
                }
                component_path = "/langflow/udf_executor"
                blob_data_to_store = blob_data
            elif component_type == "ChatOutput":
                # ChatOutput should pass through its input - create special UDF
                blob_data = self._create_chat_output_udf(node)
                step_input = {
                    "blob_id": f"udf_{step_id}",
                    "input": self._extract_chat_output_mapping(node, dependencies.get(node_id, []))
                }
                component_path = "/langflow/udf_executor"
                blob_data_to_store = blob_data
            elif custom_code:
                # Custom component - use UDF executor
                blob_data = self._prepare_udf_blob(node, component_type)
                
                step_input = {
                    "blob_id": f"udf_{step_id}",  # Will be replaced with actual blob ID
                    "input": self._extract_runtime_inputs(node, dependencies.get(node_id, []))
                }
                
                component_path = "/langflow/udf_executor"
                
                # Store blob data for later storage
                blob_data_to_store = blob_data
            else:
                # Built-in component - use Langflow component directly
                step_input = self._extract_component_inputs(node, dependencies.get(node_id, []))
                component_path = f"/langflow/{component_type}"
                blob_data_to_store = None
            
            # Extract output schema
            output_schema = self.schema_mapper.extract_output_schema(node)
            
            # Create Stepflow step
            step = StepflowStep(
                id=step_id,
                component=component_path,
                input=step_input,
                output_schema=output_schema,
                # Dependencies will be handled at the workflow level
            )
            
            # Store blob data for later storage (only for UDF components)
            if blob_data_to_store:
                step._udf_blob_data = blob_data_to_store  # type: ignore
            
            return step
            
        except Exception as e:
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
        for field_name, field_config in template.items():
            if field_name != "code":
                prepared_template[field_name] = field_config
        
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
    
    def _extract_component_inputs(
        self, 
        node: Dict[str, Any], 
        dependency_node_ids: List[str]
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
        # For now, use a simple mapping - this should be improved with actual edge analysis
        if dependency_node_ids:
            # Map first dependency to common input field names
            dep_step_id = self._generate_step_id(dependency_node_ids[0], "")
            
            # Common input field names for different component types
            component_type = node_data.get("type", "")
            if component_type in ["ChatOutput", "TextOutput"]:
                component_inputs["input_value"] = {
                    "$from": {"step": dep_step_id},
                    "path": "result"
                }
            elif component_type in ["LanguageModelComponent", "OpenAIModelComponent"]:
                component_inputs["input_value"] = {
                    "$from": {"step": dep_step_id},
                    "path": "result"
                }
            else:
                # Generic mapping
                component_inputs["input"] = {
                    "$from": {"step": dep_step_id},
                    "path": "result"
                }
        
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
        Output(display_name="Message", name="message", method="process_message")
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
            "outputs": [{"name": "message", "method": "process_message", "types": ["Message"]}],
            "selected_output": "message"
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
        HandleInput(name="message", display_name="Input Message", input_types=["Message", "str"])
    ]
    
    outputs = [
        Output(display_name="Message", name="message", method="process_output")
    ]

    def process_output(self) -> Message:
        """Process the output message."""
        input_msg = self.message
        
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
                "message": {
                    "type": "Message",
                    "value": None,
                    "info": "Input message to output"
                }
            },
            "outputs": [{"name": "message", "method": "process_output", "types": ["Message"]}],
            "selected_output": "message"
        }