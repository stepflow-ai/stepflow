// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use std::sync::Arc;

use error_stack::ResultExt as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{
    Context, DynPlugin, ExecutionContext, Plugin, PluginConfig, PluginError, Result,
};
use stepflow_protocol::stdio::StdioError;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::RwLock;
use tokio::time::{Duration, timeout};

#[allow(unused_imports)]
use crate::protocol::{
    INITIALIZE_METHOD, Implementation, ServerCapabilities, TOOLS_CALL_METHOD, TOOLS_LIST_METHOD,
    Tool,
};
use crate::schema::{component_url_to_tool_name, mcp_tool_to_component_info};

#[derive(Serialize, Deserialize, Debug)]
pub struct McpPluginConfig {
    pub command: String,
    pub args: Vec<String>,
    /// Environment variables to pass to the MCP server process.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,
}

impl PluginConfig for McpPluginConfig {
    type Error = StdioError;

    async fn create_plugin(
        self,
        working_directory: &std::path::Path,
        protocol_prefix: &str,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        // Create MCP plugin directly without StdioPlugin delegation
        Ok(DynPlugin::boxed(McpPlugin::new(
            self,
            working_directory.to_owned(),
            protocol_prefix.to_string(),
        )))
    }
}

pub struct McpPlugin {
    state: RwLock<McpPluginState>,
    config: McpPluginConfig,
    working_directory: std::path::PathBuf,
    plugin_name: String,
}

/// Dedicated MCP client for handling JSON-RPC communication
#[derive(Debug)]
struct McpClient {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    async fn new(config: &McpPluginConfig, working_directory: &std::path::Path) -> Result<Self> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        cmd.current_dir(working_directory);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Set environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let mut process = cmd.spawn().map_err(|e| {
            error_stack::Report::new(PluginError::Initializing)
                .attach_printable(format!("Failed to spawn MCP server process: {e}"))
        })?;

        let stdin = process.stdin.take().ok_or_else(|| {
            error_stack::Report::new(PluginError::Initializing)
                .attach_printable("Failed to capture stdin for MCP server")
        })?;

        let stdout = process.stdout.take().ok_or_else(|| {
            error_stack::Report::new(PluginError::Initializing)
                .attach_printable("Failed to capture stdout for MCP server")
        })?;

        Ok(Self {
            process,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id
        });

        let msg = serde_json::to_string(&request).map_err(|e| {
            error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                "Failed to serialize MCP request for method '{method}': {e}"
            ))
        })?;

        // Send the request with timeout
        let send_timeout = Duration::from_secs(5);
        timeout(send_timeout, async {
            self.stdin.write_all(msg.as_bytes()).await?;
            self.stdin.write_all(b"\n").await?;
            self.stdin.flush().await
        })
        .await
        .map_err(|_| {
            error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                "Timeout sending request to MCP server for method '{method}'"
            ))
        })?
        .map_err(|e| {
            error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                "Failed to write to MCP server for method '{method}': {e}"
            ))
        })?;

        // Read the response with timeout
        let read_timeout = Duration::from_secs(30); // Longer timeout for tool execution
        let mut line = String::new();
        let bytes_read = timeout(read_timeout, self.stdout.read_line(&mut line))
            .await
            .map_err(|_| {
                error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                    "Timeout waiting for MCP server response for method '{method}'"
                ))
            })?
            .map_err(|e| {
                error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                    "Failed to read from MCP server for method '{method}': {e}"
                ))
            })?;

        if bytes_read == 0 {
            return Err(
                error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                    "MCP server closed connection while waiting for response to method '{method}'"
                )),
            );
        }

        // Parse the response
        let response: serde_json::Value = serde_json::from_str(line.trim()).map_err(|e| {
            error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                "Failed to parse MCP server response for method '{}': {}. Raw response: '{}'",
                method,
                e,
                line.trim()
            ))
        })?;

        // Validate response ID matches request
        if let Some(response_id) = response.get("id") {
            if response_id.as_u64() != Some(id) {
                return Err(error_stack::Report::new(PluginError::Execution)
                    .attach_printable(format!("Response ID mismatch for method '{method}': expected {id}, got {response_id}")));
            }
        }

        // Check for JSON-RPC errors
        if let Some(error) = response.get("error") {
            let error_code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            let error_message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            let error_data = error
                .get("data")
                .map(|d| format!(", data: {d}"))
                .unwrap_or_default();

            return Err(error_stack::Report::new(PluginError::Execution)
                .attach_printable(format!("MCP server error for method '{method}': [{error_code}] {error_message}{error_data}")));
        }

        // Return the result field
        response.get("result").cloned().ok_or_else(|| {
            error_stack::Report::new(PluginError::Execution)
                .attach_printable(format!("MCP server response for method '{method}' missing result field. Response: {response}"))
        })
    }

    async fn send_notification(&mut self, method: &str, params: serde_json::Value) -> Result<()> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let msg = serde_json::to_string(&request).map_err(|e| {
            error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                "Failed to serialize MCP notification for method '{method}': {e}"
            ))
        })?;

        // Send the notification with timeout
        let send_timeout = Duration::from_secs(5);
        timeout(send_timeout, async {
            self.stdin.write_all(msg.as_bytes()).await?;
            self.stdin.write_all(b"\n").await?;
            self.stdin.flush().await
        })
        .await
        .map_err(|_| {
            error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                "Timeout sending notification to MCP server for method '{method}'"
            ))
        })?
        .map_err(|e| {
            error_stack::Report::new(PluginError::Execution).attach_printable(format!(
                "Failed to write notification to MCP server for method '{method}': {e}"
            ))
        })?;

        Ok(())
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Try to gracefully terminate the MCP server process
        // Note: We ignore errors here as this is best-effort cleanup during drop
        let _ = self.process.start_kill();
    }
}

#[derive(Debug)]
struct McpPluginState {
    server_info: Option<Implementation>,
    server_capabilities: Option<ServerCapabilities>,
    available_tools: Vec<Tool>,
    mcp_client: Option<McpClient>,
}

impl McpPlugin {
    pub fn new(
        config: McpPluginConfig,
        working_directory: std::path::PathBuf,
        plugin_name: String,
    ) -> Self {
        Self {
            state: RwLock::new(McpPluginState {
                server_info: None,
                server_capabilities: None,
                available_tools: Vec::new(),
                mcp_client: None,
            }),
            config,
            working_directory,
            plugin_name,
        }
    }
}

impl Plugin for McpPlugin {
    async fn init(&self, _context: &Arc<dyn Context>) -> Result<()> {
        // Don't initialize the underlying stdio plugin since we handle MCP protocol directly

        // Create the MCP client
        let mcp_client = McpClient::new(&self.config, &self.working_directory).await?;

        // Store the client in state
        {
            let mut state = self.state.write().await;
            state.mcp_client = Some(mcp_client);
        }

        // Now perform MCP-specific initialization
        self.perform_mcp_handshake().await?;

        // Discover available tools
        self.discover_tools().await?;

        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        let state = self.state.read().await;
        let mut components = Vec::new();

        // Convert MCP tools to StepFlow components
        for tool in &state.available_tools {
            let mut info = crate::schema::mcp_tool_to_component_info(tool)
                .change_context(PluginError::ComponentInfo)?;

            // Update the component URL to use plugin name as protocol
            let component_url = format!("{}://{}", self.plugin_name, tool.name);
            info.component = Component::from_string(&component_url);

            components.push(info);
        }

        Ok(components)
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let tool_name = component_url_to_tool_name(component.url_string())
            .ok_or(PluginError::ComponentInfo)
            .attach_printable("Invalid MCP component URL format")?;

        let state = self.state.read().await;
        let tool = state
            .available_tools
            .iter()
            .find(|tool| tool.name == tool_name)
            .ok_or(PluginError::ComponentInfo)
            .attach_printable("MCP tool not found")?;

        mcp_tool_to_component_info(tool).change_context(PluginError::ComponentInfo)
    }

    async fn execute(
        &self,
        component: &Component,
        _context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let tool_name = component_url_to_tool_name(component.url_string())
            .ok_or(PluginError::Execution)
            .attach_printable("Invalid MCP component URL format")?;

        let mut state = self.state.write().await;
        let mcp_client = state.mcp_client.as_mut().ok_or_else(|| {
            error_stack::Report::new(PluginError::Execution)
                .attach_printable("MCP client not initialized")
        })?;

        // Send tools/call request to execute the tool
        let call_params = json!({
            "name": tool_name,
            "arguments": input.clone_value()
        });

        let call_result = mcp_client.send_request("tools/call", call_params).await?;

        // The result should be in the "content" field according to MCP spec
        let content = call_result.get("content").cloned().unwrap_or_else(|| {
            // Fallback to the entire result if content field is missing
            call_result.clone()
        });

        Ok(FlowResult::Success {
            result: ValueRef::new(content),
        })
    }
}

impl McpPlugin {
    // MCP-specific helper methods
    async fn perform_mcp_handshake(&self) -> Result<()> {
        // Step 1: Send initialize request
        let initialize_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": {
                    "listChanged": false
                },
                "sampling": {}
            },
            "clientInfo": {
                "name": "stepflow",
                "version": "1.0.0"
            }
        });

        let initialize_result = {
            let mut state = self.state.write().await;
            let mcp_client = state.mcp_client.as_mut().ok_or_else(|| {
                error_stack::Report::new(PluginError::Initializing)
                    .attach_printable("MCP client not initialized")
            })?;
            mcp_client
                .send_request("initialize", initialize_params)
                .await?
        };

        // Parse and store the server capabilities and info from the response
        {
            let mut state = self.state.write().await;

            if let Some(capabilities) = initialize_result.get("capabilities") {
                let server_capabilities: ServerCapabilities =
                    serde_json::from_value(capabilities.clone()).map_err(|e| {
                        error_stack::Report::new(PluginError::Initializing)
                            .attach_printable(format!("Failed to parse server capabilities: {e}"))
                    })?;
                state.server_capabilities = Some(server_capabilities);
            }

            if let Some(server_info) = initialize_result.get("serverInfo") {
                let implementation: Implementation = serde_json::from_value(server_info.clone())
                    .map_err(|e| {
                        error_stack::Report::new(PluginError::Initializing)
                            .attach_printable(format!("Failed to parse server info: {e}"))
                    })?;
                state.server_info = Some(implementation);
            }
        }

        // Step 2: Send initialized notification
        {
            let mut state = self.state.write().await;
            let mcp_client = state.mcp_client.as_mut().ok_or_else(|| {
                error_stack::Report::new(PluginError::Initializing)
                    .attach_printable("MCP client not initialized")
            })?;
            mcp_client
                .send_notification("notifications/initialized", json!({}))
                .await?;
        }

        Ok(())
    }

    async fn discover_tools(&self) -> Result<()> {
        // Send tools/list request to discover available tools
        let tools_result = {
            let mut state = self.state.write().await;
            let mcp_client = state.mcp_client.as_mut().ok_or_else(|| {
                error_stack::Report::new(PluginError::Initializing)
                    .attach_printable("MCP client not initialized")
            })?;
            mcp_client.send_request("tools/list", json!({})).await?
        };

        // Parse the tools from the response
        let tools = if let Some(tools_array) = tools_result.get("tools") {
            let tools_vec: Vec<Tool> =
                serde_json::from_value(tools_array.clone()).map_err(|e| {
                    error_stack::Report::new(PluginError::Initializing)
                        .attach_printable(format!("Failed to parse tools list: {e}"))
                })?;
            tools_vec
        } else {
            Vec::new()
        };

        // Store the tools
        {
            let mut state = self.state.write().await;
            state.available_tools = tools;
        }

        Ok(())
    }
}
