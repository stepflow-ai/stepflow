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

use serde::{Deserialize, Serialize};

use crate::{BuiltinPluginConfig, GrpcPluginConfig, McpPluginConfig, NatsPluginConfig};

#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
#[schemars(transform = stepflow_flow::discriminator_schema::AddDiscriminator::new("type"))]
pub enum SupportedPlugin {
    #[schemars(title = "BuiltinPluginConfig")]
    Builtin(BuiltinPluginConfig),
    /// Mock plugin configuration (opaque — deserialized by the server).
    ///
    /// Uses `serde_yaml_ng::Value` because mock configs can contain non-string
    /// map keys (e.g., `{input: "a"}` as behavior keys) which `serde_json::Value`
    /// cannot represent.
    #[schemars(title = "MockPlugin", with = "serde_json::Value")]
    Mock(serde_yaml_ng::Value),
    #[schemars(title = "McpPluginConfig")]
    Mcp(McpPluginConfig),
    #[schemars(title = "GrpcPluginConfig")]
    Grpc(GrpcPluginConfig),
    #[schemars(title = "NatsPluginConfig")]
    Nats(NatsPluginConfig),
}

#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema)]
pub struct SupportedPluginConfig {
    #[serde(flatten)]
    pub plugin: SupportedPlugin,
}
