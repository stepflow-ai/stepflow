#!/usr/bin/env python3
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
Mock MCP server for testing the Stepflow MCP plugin.
This server implements the basic MCP protocol over stdio.
"""

import json
import sys


def send_response(id, result):
    """Send a JSON-RPC response."""
    response = {"jsonrpc": "2.0", "id": id, "result": result}
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()


def send_error(id, code, message):
    """Send a JSON-RPC error response."""
    response = {"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()


def main():
    """Main loop for the mock MCP server."""
    while True:
        try:
            line = sys.stdin.readline()
            if not line:
                break

            request = json.loads(line.strip())
            method = request.get("method")
            params = request.get("params", {})
            request_id = request.get("id")

            if method == "initialize":
                # Respond to initialization
                result = {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {},
                        "resources": None,
                        "prompts": None,
                        "logging": None,
                    },
                    "serverInfo": {"name": "mock-mcp-server", "version": "1.0.0"},
                }
                send_response(request_id, result)

            elif method == "notifications/initialized":
                # This is a notification, no response needed
                pass

            elif method == "tools/list":
                # Return a list of available tools
                result = {
                    "tools": [
                        {
                            "name": "echo",
                            "description": "Echoes the input back",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "message": {
                                        "type": "string",
                                        "description": "Message to echo",
                                    }
                                },
                                "required": ["message"],
                            },
                        },
                        {
                            "name": "add",
                            "description": "Adds two numbers",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "a": {
                                        "type": "number",
                                        "description": "First number",
                                    },
                                    "b": {
                                        "type": "number",
                                        "description": "Second number",
                                    },
                                },
                                "required": ["a", "b"],
                            },
                        },
                    ]
                }
                send_response(request_id, result)

            elif method == "tools/call":
                # Execute a tool
                tool_name = params.get("name")
                arguments = params.get("arguments", {})

                if tool_name == "echo":
                    message = arguments.get("message", "")
                    result = {"content": [{"type": "text", "text": f"Echo: {message}"}]}
                    send_response(request_id, result)

                elif tool_name == "add":
                    a = arguments.get("a", 0)
                    b = arguments.get("b", 0)
                    result = {"content": [{"type": "text", "text": f"Result: {a + b}"}]}
                    send_response(request_id, result)

                else:
                    send_error(request_id, -32601, f"Unknown tool: {tool_name}")

            else:
                send_error(request_id, -32601, f"Method not found: {method}")

        except json.JSONDecodeError as e:
            # Invalid JSON
            send_error(None, -32700, f"Parse error: {str(e)}")
        except Exception as e:
            # Other errors
            send_error(None, -32603, f"Internal error: {str(e)}")


if __name__ == "__main__":
    main()
