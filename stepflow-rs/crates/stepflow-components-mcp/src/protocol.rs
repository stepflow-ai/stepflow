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
