use std::sync::Arc;

use error_stack::ResultExt as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{
    Context, DynPlugin, ExecutionContext, Plugin, PluginConfig, PluginError, Result,
};
use stepflow_protocol::stdio::{StdioError, StdioPluginConfig};
use tokio::sync::RwLock;

#[allow(unused_imports)]
use crate::protocol::{INITIALIZE_METHOD, TOOLS_CALL_METHOD, TOOLS_LIST_METHOD};
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
        // Convert McpPluginConfig to StdioPluginConfig to reuse existing infrastructure
        let stdio_config = StdioPluginConfig {
            command: self.command,
            args: self.args,
            env: self.env,
        };

        // Create underlying StdioPlugin
        let stdio_plugin = stdio_config
            .create_plugin(working_directory, protocol_prefix)
            .await?;

        // Wrap it with MCP-specific logic
        Ok(DynPlugin::boxed(McpPlugin::new(stdio_plugin)))
    }
}

pub struct McpPlugin {
    stdio_plugin: Box<DynPlugin<'static>>,
    state: RwLock<McpPluginState>,
}

#[derive(Debug)]
struct McpPluginState {
    server_info: Option<JsonValue>,
    available_tools: Vec<JsonValue>,
}

impl McpPlugin {
    pub fn new(stdio_plugin: Box<DynPlugin<'static>>) -> Self {
        Self {
            stdio_plugin,
            state: RwLock::new(McpPluginState {
                server_info: None,
                available_tools: Vec::new(),
            }),
        }
    }
}

impl Plugin for McpPlugin {
    async fn init(&self, context: &Arc<dyn Context>) -> Result<()> {
        // First initialize the underlying stdio plugin
        self.stdio_plugin.init(context).await?;

        // Now perform MCP-specific initialization
        self.perform_mcp_handshake().await?;

        // Discover available tools
        self.discover_tools().await?;

        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<Component>> {
        let state = self.state.read().await;
        let mut components = Vec::new();

        // Convert MCP tools to StepFlow components
        for tool in &state.available_tools {
            if let Some(name) = tool.get("name").and_then(|n| n.as_str()) {
                // Create component URL in format: mcp+stdio://server_name/tool_name
                let component_url = format!("mcp+stdio://server/{}", name);
                components.push(Component::from_string(&component_url));
            }
        }

        Ok(components)
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let tool_name = component_url_to_tool_name(component.url_string())
            .ok_or(PluginError::ComponentInfo)
            .attach_printable("Invalid MCP component URL format")?;

        let state = self.state.read().await;
        let tool_schema = state
            .available_tools
            .iter()
            .find(|tool| tool.get("name").and_then(|n| n.as_str()) == Some(&tool_name))
            .ok_or(PluginError::ComponentInfo)
            .attach_printable("MCP tool not found")?;

        mcp_tool_to_component_info(&tool_name, tool_schema)
            .change_context(PluginError::ComponentInfo)
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

        // For now, return a placeholder success result
        // TODO: Implement actual MCP tool execution
        let result = json!({
            "tool": tool_name,
            "input": input.clone_value(),
            "message": "MCP tool execution not yet implemented"
        });

        Ok(FlowResult::Success {
            result: ValueRef::new(result),
        })
    }
}

impl McpPlugin {
    // MCP-specific helper methods
    async fn perform_mcp_handshake(&self) -> Result<()> {
        // For now, this is a placeholder for MCP handshake
        // TODO: Implement actual MCP initialize/initialized sequence
        // This would involve:
        // 1. Sending MCP initialize request with client capabilities
        // 2. Receiving server capabilities in response
        // 3. Sending initialized notification

        let mut state = self.state.write().await;
        state.server_info = Some(json!({
            "name": "MCP Server",
            "version": "1.0.0",
            "capabilities": {
                "tools": {}
            }
        }));

        Ok(())
    }

    async fn discover_tools(&self) -> Result<()> {
        // For now, this is a placeholder for tool discovery
        // TODO: Implement actual MCP tools/list request
        // This would involve sending a tools/list request to the MCP server
        // and parsing the response to extract available tools

        let mut state = self.state.write().await;
        state.available_tools = vec![json!({
            "name": "example_tool",
            "description": "An example MCP tool",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input text"
                    }
                },
                "required": ["input"]
            }
        })];

        Ok(())
    }
}
