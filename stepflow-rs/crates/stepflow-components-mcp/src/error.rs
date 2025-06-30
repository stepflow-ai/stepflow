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
