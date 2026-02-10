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

use std::path::Path;

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_builtins::BuiltinPluginConfig;
use stepflow_components_mcp::McpPluginConfig;
use stepflow_mock::MockPlugin;
use stepflow_plugin::{DynPlugin, PluginConfig};
use stepflow_protocol::StepflowPluginConfig;

use crate::{ConfigError, Result};

#[derive(Serialize, Deserialize, Debug, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SupportedPlugin {
    Stepflow(StepflowPluginConfig),
    Builtin(BuiltinPluginConfig),
    Mock(MockPlugin),
    Mcp(McpPluginConfig),
}

#[derive(Serialize, Deserialize, Debug, utoipa::ToSchema)]
pub struct SupportedPluginConfig {
    #[serde(flatten)]
    pub plugin: SupportedPlugin,
}

async fn create_plugin<P: PluginConfig>(
    plugin: P,
    working_directory: &Path,
) -> Result<Box<DynPlugin<'static>>> {
    plugin
        .create_plugin(working_directory)
        .await
        .change_context(ConfigError::Configuration)
}

impl SupportedPluginConfig {
    pub async fn instantiate(self, working_directory: &Path) -> Result<Box<DynPlugin<'static>>> {
        let plugin = match self.plugin {
            SupportedPlugin::Stepflow(plugin) => create_plugin(plugin, working_directory).await?,
            SupportedPlugin::Builtin(plugin) => create_plugin(plugin, working_directory).await?,
            SupportedPlugin::Mock(plugin) => create_plugin(plugin, working_directory).await?,
            SupportedPlugin::Mcp(plugin) => create_plugin(plugin, working_directory).await?,
        };
        Ok(plugin)
    }
}
