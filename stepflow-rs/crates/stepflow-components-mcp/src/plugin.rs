use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{Context, DynPlugin, ExecutionContext, Plugin, PluginConfig, Result};
use stepflow_protocol::stdio::{StdioError, StdioPluginConfig};

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
}

impl McpPlugin {
    pub fn new(stdio_plugin: Box<DynPlugin<'static>>) -> Self {
        Self { stdio_plugin }
    }
}

impl Plugin for McpPlugin {
    async fn init(&self, context: &Arc<dyn Context>) -> Result<()> {
        // Delegate to underlying stdio plugin for now
        // Later we'll add MCP-specific initialization
        self.stdio_plugin.init(context).await
    }

    async fn list_components(&self) -> Result<Vec<Component>> {
        // TODO: Query MCP server for available tools and convert to Components
        // For now, delegate to stdio plugin (which will return empty list)
        self.stdio_plugin.list_components().await
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        // TODO: Get tool schema from MCP server and convert to ComponentInfo
        // For now, delegate to stdio plugin
        self.stdio_plugin.component_info(component).await
    }

    async fn execute(
        &self,
        component: &Component,
        context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult> {
        // TODO: Execute MCP tool and convert result to FlowResult
        // For now, delegate to stdio plugin
        self.stdio_plugin.execute(component, context, input).await
    }
}
