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

// Schema conversion utilities for MCP tools
// This module handles converting MCP tool schemas to StepFlow ComponentInfo

use crate::error::{McpError, Result};
use stepflow_core::{component::ComponentInfo, schema::SchemaRef, workflow::Component};

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

/// Convert StepFlow Component path to MCP tool name
pub fn component_path_to_tool_name(component_path: &str) -> Option<String> {
    // Expected format: /plugin_name/tool_name
    if let Some(without_leading_slash) = component_path.strip_prefix('/') {
        if let Some(pos) = without_leading_slash.find('/') {
            return Some(without_leading_slash[pos + 1..].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_path_to_tool_name() {
        assert_eq!(
            component_path_to_tool_name("/filesystem/read_file"),
            Some("read_file".to_string())
        );

        assert_eq!(
            component_path_to_tool_name("/mock-server/tool_name"),
            Some("tool_name".to_string())
        );

        assert_eq!(component_path_to_tool_name("invalidpath"), None);
        assert_eq!(component_path_to_tool_name("/filesystem"), None);
    }
}
