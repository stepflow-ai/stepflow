// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

// Schema conversion utilities for MCP tools
// This module handles converting MCP tool schemas to Stepflow ComponentInfo

use crate::error::{McpError, Result};
use stepflow_core::{component::ComponentInfo, schema::SchemaRef, workflow::Component};

// Import the official Tool struct from rust-mcp-schema
use crate::protocol::Tool;

/// Convert MCP tool to Stepflow ComponentInfo
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

    // Create component path in MCP-compliant format: /mcp/server/tool_name
    // Note: The actual server name will be injected by the plugin when listing components
    let component_path = format!("/mcp/server/{}", tool.name);
    let component = Component::from_string(&component_path);

    Ok(ComponentInfo {
        component,
        description: Some(description),
        input_schema: Some(input_schema),
        output_schema: Some(output_schema),
    })
}

/// Convert Stepflow Component path to MCP tool name
pub fn component_path_to_tool_name(component_path: &str) -> Option<String> {
    // After routing, we get paths like "/write_file" or "/plugin_name/tool_name"
    if let Some(without_leading_slash) = component_path.strip_prefix('/') {
        // If it contains another slash, extract the part after the last slash
        if let Some(pos) = without_leading_slash.rfind('/') {
            return Some(without_leading_slash[pos + 1..].to_string());
        } else {
            // No additional slash, so this is already just the tool name
            return Some(without_leading_slash.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_path_to_tool_name() {
        // Test routed paths (after routing removes plugin prefix)
        assert_eq!(
            component_path_to_tool_name("/write_file"),
            Some("write_file".to_string())
        );

        assert_eq!(
            component_path_to_tool_name("/read_text_file"),
            Some("read_text_file".to_string())
        );

        // Test full paths (before routing)
        assert_eq!(
            component_path_to_tool_name("/filesystem/read_file"),
            Some("read_file".to_string())
        );

        assert_eq!(
            component_path_to_tool_name("/mock-server/tool_name"),
            Some("tool_name".to_string())
        );

        // Test nested paths
        assert_eq!(
            component_path_to_tool_name("/plugin/sub/tool_name"),
            Some("tool_name".to_string())
        );

        // Test invalid paths
        assert_eq!(component_path_to_tool_name("invalidpath"), None);
        assert_eq!(component_path_to_tool_name(""), None);
    }
}
