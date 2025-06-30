use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum McpError {
    #[error("MCP server initialization failed")]
    Initialization,

    #[error("MCP server communication failed")]
    Communication,

    #[error("MCP tool not found: {tool_name}")]
    ToolNotFound { tool_name: String },

    #[error("MCP tool execution failed")]
    ToolExecution,

    #[error("Schema conversion failed")]
    SchemaConversion,

    #[error("Invalid MCP response")]
    InvalidResponse,
}

#[allow(dead_code)]
pub type Result<T, E = error_stack::Report<McpError>> = std::result::Result<T, E>;
