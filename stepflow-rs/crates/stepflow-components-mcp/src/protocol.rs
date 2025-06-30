// MCP protocol message definitions using the official rust-mcp-schema crate
// This provides proper type definitions following the Model Context Protocol specification

// Import the main types from rust-mcp-schema
#[allow(unused_imports)]
pub use rust_mcp_schema::{
    JsonrpcMessage,
    schema_utils::{
        // Access specific message types through schema_utils
        ClientMessage, ServerMessage, MessageTypes
    },
};

// Define MCP method constants for our implementation
#[allow(dead_code)]
pub const INITIALIZE_METHOD: &str = "initialize";
#[allow(dead_code)]
pub const TOOLS_LIST_METHOD: &str = "tools/list";
#[allow(dead_code)]
pub const TOOLS_CALL_METHOD: &str = "tools/call";
