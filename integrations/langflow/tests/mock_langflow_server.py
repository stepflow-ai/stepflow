#!/usr/bin/env python3
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

"""
Mock Langflow component server for testing.

This server provides mock implementations of Langflow components that return
predictable responses for testing the integration without requiring a full
Langflow installation.
"""

import json
import sys
import asyncio
from typing import Dict, Any, Optional


class MockStepflowContext:
    """Mock implementation of StepflowContext for testing."""
    
    def __init__(self):
        self.blobs = {}
    
    async def get_blob(self, blob_id: str) -> Dict[str, Any]:
        """Retrieve blob data by ID."""
        if blob_id not in self.blobs:
            # Return default mock blob structure
            return {
                "code": f"# Mock component for {blob_id}",
                "component_type": "MockComponent",
                "template": {},
                "outputs": [{"name": "output", "method": "run", "types": ["Data"]}],
                "selected_output": "output"
            }
        return self.blobs[blob_id]
    
    async def put_blob(self, data: Dict[str, Any]) -> str:
        """Store blob data and return ID."""
        blob_id = f"mock_blob_{len(self.blobs)}"
        self.blobs[blob_id] = data
        return blob_id


class MockLangflowServer:
    """Mock server that responds to component requests with predictable data."""
    
    def __init__(self):
        self.context = MockStepflowContext()
        # Store flow-specific configurations by flow_id
        self.flow_configs = {}
    
    def get_component_info(self, component_name: str) -> Dict[str, Any]:
        """Return mock component info."""
        return {
            "name": component_name,
            "description": f"Mock {component_name} component for testing",
            "input_schema": {
                "type": "object",
                "properties": {
                    "input": {"type": "object"}
                }
            },
            "output_schema": {
                "type": "object",
                "properties": {
                    "result": {"type": "object"}
                }
            }
        }
    
    async def execute_component(self, component_name: str, input_data: Dict[str, Any], flow_id: str = "unknown") -> Dict[str, Any]:
        """Execute mock component and return predictable result."""
        
        # Check if we have flow-specific mock configuration
        flow_config = self.flow_configs.get(flow_id, {})
        
        # Extract blob_id and input from the request
        blob_id = input_data.get("blob_id", "")
        component_input = input_data.get("input", {})
        
        # Mock different component types
        if "chatinput" in component_name.lower() or "chat_input" in component_name.lower():
            # Handle the new chat_input format that reads from workflow input
            message_text = "Mock user input"
            if "message" in component_input:
                message_text = component_input.get("message", "Mock user input") 
            elif "input_value" in component_input:
                message_text = component_input.get("input_value", "Mock user input")
            
            return {
                "result": {
                    "text": message_text,
                    "sender": component_input.get("sender", "User"),
                    "sender_name": component_input.get("sender", "User"),
                    "type": "Message",
                    "__langflow_type__": "Message"
                }
            }
        elif "chatoutput" in component_name.lower() or "chat_output" in component_name.lower():
            # ChatOutput typically receives input from previous steps
            input_text = "Mock AI response"
            if "message" in component_input:
                # New chat_output format
                message_input = component_input["message"]
                if isinstance(message_input, dict) and "text" in message_input:
                    input_text = f"AI Response to: {message_input['text']}"
                elif isinstance(message_input, str):
                    input_text = f"AI Response to: {message_input}"
            elif "input_0" in component_input:
                # Old format
                prev_result = component_input["input_0"]
                if isinstance(prev_result, dict) and "text" in prev_result:
                    input_text = f"AI Response to: {prev_result['text']}"
            
            return {
                "result": {
                    "text": input_text,
                    "sender": "AI", 
                    "sender_name": "Assistant",
                    "type": "Message",
                    "__langflow_type__": "Message"
                }
            }
        elif "languagemodel" in component_name.lower():
            # Language model component - combine inputs and generate response
            input_text = "Hello"
            if "input_0" in component_input:
                prev_result = component_input["input_0"]
                if isinstance(prev_result, dict) and "text" in prev_result:
                    input_text = prev_result["text"]
            
            return {
                "result": {
                    "text": f"AI response to: {input_text}",
                    "sender": "AI",
                    "sender_name": "LanguageModel", 
                    "type": "Message",
                    "__langflow_type__": "Message"
                }
            }
        elif "prompt" in component_name.lower():
            return {
                "result": {
                    "template": component_input.get("template", "Mock prompt template"),
                    "formatted": "Mock formatted prompt"
                }
            }
        elif "note" in component_name.lower():
            return {
                "result": {
                    "note": component_input.get("text", "Mock note content")
                }
            }
        else:
            # Generic mock response
            return {
                "result": {
                    "output": f"Mock output from {component_name}",
                    "input_received": component_input
                }
            }


async def handle_request(request: Dict[str, Any]) -> Dict[str, Any]:
    """Handle incoming JSON-RPC request."""
    server = MockLangflowServer()
    
    method = request.get("method", "")
    params = request.get("params", {})
    request_id = request.get("id")
    
    # Log all incoming requests for debugging
    print(f"DEBUG: Received method '{method}' with id '{request_id}' and params {params}", file=sys.stderr, flush=True)
    
    try:
        if method == "initialize" or method == "initialized":
            result = {
                "status": "initialized", 
                "version": "mock-1.0.0",
                "server_protocol_version": 1
            }
            # Special handling for 'initialized' notifications - no response needed
            if method == "initialized" and request_id is None:
                print(f"DEBUG: Ignoring 'initialized' notification", file=sys.stderr, flush=True)
                return None  # Don't send a response for notifications
        elif method == "components/info":
            # ComponentInfoParams has a 'component' field 
            component_name = params.get("component", "unknown")
            result = {"info": server.get_component_info(component_name)}
        elif method == "components/execute":
            # ComponentExecuteParams structure: component, input, step_id, run_id, flow_id
            component_name = params.get("component", "unknown")
            input_data = params.get("input", {})
            step_id = params.get("step_id", "unknown")
            run_id = params.get("run_id", "unknown")
            flow_id = params.get("flow_id", "unknown")
            # Log the component execution for debugging
            print(f"DEBUG: Executing component {component_name} (step: {step_id}, flow: {flow_id}) with input {input_data}", file=sys.stderr, flush=True)
            execution_result = await server.execute_component(component_name, input_data, flow_id=flow_id)
            # ComponentExecuteResult expects 'output' field
            result = {"output": execution_result}
        elif method == "components/list":
            # Return list of available components (use 'component' field, not 'name')
            result = {"components": [
                {"component": "/ChatInput", "description": "Mock chat input component"},
                {"component": "/ChatOutput", "description": "Mock chat output component"},
                {"component": "/chat_input", "description": "Mock chat input component (new format)"},
                {"component": "/chat_output", "description": "Mock chat output component (new format)"},
                {"component": "/udf_executor", "description": "Mock UDF executor component"},
                {"component": "/note", "description": "Mock note component"},
            ]}
        else:
            # Instead of raising exception, return a default response
            print(f"WARNING: Unknown method '{method}', returning empty result", file=sys.stderr, flush=True)
            result = {"status": "ok", "message": f"Mock server handled unknown method: {method}"}
        
        return {
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result
        }
    
    except Exception as e:
        return {
            "jsonrpc": "2.0", 
            "id": request_id,
            "error": {
                "code": -1,
                "message": str(e)
            }
        }


async def main():
    """Main server loop - read JSON-RPC requests from stdin."""
    try:
        while True:
            line = sys.stdin.readline()
            if not line:
                break
            
            line = line.strip()
            if not line:
                continue
            
            try:
                request = json.loads(line)
                response = await handle_request(request)
                # Only send response if one was returned (not None for notifications)
                if response is not None:
                    print(json.dumps(response), flush=True)
            except json.JSONDecodeError:
                # Send error response for invalid JSON
                error_response = {
                    "jsonrpc": "2.0",
                    "id": None,
                    "error": {
                        "code": -32700,
                        "message": "Parse error"
                    }
                }
                print(json.dumps(error_response), flush=True)
    
    except KeyboardInterrupt:
        pass
    except EOFError:
        pass


if __name__ == "__main__":
    asyncio.run(main())