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

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Configuration for an MCP (Model Context Protocol) plugin.
#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema)]
pub struct McpPluginConfig {
    pub command: String,
    pub args: Vec<String>,
    /// Environment variables to pass to the MCP server process.
    /// Values can contain environment variable references like ${HOME} or ${USER:-default}.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    #[schemars(with = "std::collections::HashMap<String, String>")]
    pub env: IndexMap<String, String>,
}
