// Schema conversion utilities for MCP tools
// This module handles converting MCP tool schemas to StepFlow ComponentInfo

use crate::error::{McpError, Result};
use stepflow_core::component::ComponentInfo;

/// Convert MCP tool schema to StepFlow ComponentInfo
pub fn mcp_tool_to_component_info(
    _tool_name: &str,
    _tool_schema: &serde_json::Value,
) -> Result<ComponentInfo> {
    // TODO: Implement actual conversion from MCP tool schema to ComponentInfo
    // This will parse MCP tool definitions and create appropriate ComponentInfo with schemas

    Err(error_stack::Report::new(McpError::SchemaConversion)
        .attach_printable("Schema conversion not yet implemented"))
}

/// Convert StepFlow Component URL to MCP tool name
pub fn component_url_to_tool_name(component_url: &str) -> Option<String> {
    // Expected format: mcp+stdio://server_name/tool_name
    if let Some(url_part) = component_url.strip_prefix("mcp+stdio://") {
        if let Some(slash_pos) = url_part.find('/') {
            return Some(url_part[slash_pos + 1..].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_url_to_tool_name() {
        assert_eq!(
            component_url_to_tool_name("mcp+stdio://filesystem/read_file"),
            Some("read_file".to_string())
        );

        assert_eq!(
            component_url_to_tool_name("mcp+stdio://server/tool_name"),
            Some("tool_name".to_string())
        );

        assert_eq!(component_url_to_tool_name("invalid://url"), None);
    }
}
