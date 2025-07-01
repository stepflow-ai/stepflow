// Schema conversion utilities for MCP tools
// This module handles converting MCP tool schemas to StepFlow ComponentInfo

use crate::error::{McpError, Result};
use stepflow_core::{component::ComponentInfo, schema::SchemaRef};

// Import the official Tool struct from rust-mcp-schema
use crate::protocol::Tool;

/// Convert MCP tool to StepFlow ComponentInfo
pub fn mcp_tool_to_component_info(tool: &Tool) -> Result<ComponentInfo> {
    // Use the description from the tool, or default
    let description = tool
        .description
        .clone()
        .unwrap_or_else(|| "MCP tool".to_string());

    // Convert the input schema from the tool
    let input_schema =
        SchemaRef::parse_json(&serde_json::to_string(&tool.input_schema).map_err(|_| {
            error_stack::Report::new(McpError::SchemaConversion)
                .attach_printable("Failed to serialize MCP input schema")
        })?)
        .map_err(|_| {
            error_stack::Report::new(McpError::SchemaConversion)
                .attach_printable("Failed to parse MCP input schema")
        })?;

    // MCP tools typically return unstructured results, so use a flexible output schema
    let output_schema = SchemaRef::parse_json(r#"{"type": "object"}"#).map_err(|_| {
        error_stack::Report::new(McpError::SchemaConversion)
            .attach_printable("Failed to create output schema")
    })?;

    Ok(ComponentInfo {
        description: Some(description),
        input_schema,
        output_schema,
    })
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
