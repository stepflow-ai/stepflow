// MCP protocol message definitions using the official rust-mcp-schema crate
// This provides proper type definitions following the Model Context Protocol specification

// Import the main types from rust-mcp-schema
#[allow(unused_imports)]
pub use rust_mcp_schema::{
    // Core types
    Implementation,
    // Initialization types
    InitializeRequest,
    InitializeRequestParams,
    InitializeResult,
    InitializedNotification,

    // Tool-related types
    ListToolsRequest,
    ListToolsResult,
    ServerCapabilities,
    Tool,

    ToolInputSchema,

    // Message types (if available through schema_utils)
    schema_utils::*,
};

// Define MCP method constants for our implementation
#[allow(dead_code)]
pub const INITIALIZE_METHOD: &str = "initialize";
#[allow(dead_code)]
pub const TOOLS_LIST_METHOD: &str = "tools/list";
#[allow(dead_code)]
pub const TOOLS_CALL_METHOD: &str = "tools/call";
