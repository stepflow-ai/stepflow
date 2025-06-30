// MCP protocol message definitions
// This module will contain the MCP-specific JSON-RPC method definitions
// following the Model Context Protocol specification

use serde::{Deserialize, Serialize};

// Placeholder for MCP protocol methods
// These will be implemented in the next phase

#[derive(Serialize, Deserialize, Debug)]
pub struct InitializeRequest {
    pub protocol_version: String,
    pub client_info: ClientInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitializeResponse {
    pub protocol_version: String,
    pub server_info: ServerInfo,
    pub capabilities: ServerCapabilities,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ToolsCapability {
    pub list_changed: Option<bool>,
}

// TODO: Add more MCP protocol types as needed
