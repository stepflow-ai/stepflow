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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use stepflow_builtins::BuiltinPluginConfig;
use stepflow_components_mcp::McpPluginConfig;
use stepflow_mock::MockPlugin;
use stepflow_plugin::routing::RoutingConfig;
use stepflow_plugin::{DynPlugin, PluginConfig};
use stepflow_protocol::StepflowPluginConfig;
use stepflow_state::{InMemoryStateStore, StateStore};
use stepflow_state_sql::{SqliteStateStore, SqliteStateStoreConfig};

#[derive(Serialize, Deserialize)]
pub struct StepflowConfig {
    /// Working directory for the configuration.
    ///
    /// If not set, this will be the directory containing the config.
    pub working_directory: Option<PathBuf>,
    pub plugins: IndexMap<String, SupportedPluginConfig>,
    /// Routing configuration for mapping components to plugins.
    #[serde(flatten)]
    pub routing: RoutingConfig,
    /// State store configuration. If not specified, uses in-memory storage.
    #[serde(default)]
    pub state_store: StateStoreConfig,
}

impl Default for StepflowConfig {
    fn default() -> Self {
        let mut plugins = IndexMap::new();
        plugins.insert(
            "builtin".to_string(),
            SupportedPluginConfig {
                plugin: SupportedPlugin::Builtin(BuiltinPluginConfig),
            },
        );

        Self {
            working_directory: None,
            plugins,
            routing: RoutingConfig::default(),
            state_store: StateStoreConfig::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StateStoreConfig {
    /// In-memory state store (default, for testing and demos)
    InMemory,
    /// SQLite-based persistent state store
    Sqlite(SqliteStateStoreConfig),
}

impl Default for StateStoreConfig {
    fn default() -> Self {
        Self::InMemory
    }
}

impl StateStoreConfig {
    /// Create a StateStore instance from this configuration
    pub async fn create_state_store(&self) -> Result<Arc<dyn StateStore>> {
        match self {
            StateStoreConfig::InMemory => Ok(Arc::new(InMemoryStateStore::new())),
            StateStoreConfig::Sqlite(config) => {
                let store = SqliteStateStore::new(config.clone())
                    .await
                    .change_context(MainError::Configuration)?;
                Ok(Arc::new(store))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
enum SupportedPlugin {
    Stepflow(StepflowPluginConfig),
    Builtin(BuiltinPluginConfig),
    Mock(MockPlugin),
    Mcp(McpPluginConfig),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SupportedPluginConfig {
    #[serde(flatten)]
    plugin: SupportedPlugin,
}

async fn create_plugin<P: PluginConfig>(
    plugin: P,
    working_directory: &Path,
    protocol_prefix: &str,
) -> Result<Box<DynPlugin<'static>>> {
    plugin
        .create_plugin(working_directory, protocol_prefix)
        .await
        .change_context(MainError::RegisterPlugin)
}

impl SupportedPluginConfig {
    pub async fn instantiate(
        self,
        working_directory: &Path,
        plugin_name: &str,
    ) -> Result<Box<DynPlugin<'static>>> {
        let plugin = match self.plugin {
            SupportedPlugin::Stepflow(plugin) => {
                create_plugin(plugin, working_directory, plugin_name).await
            }
            SupportedPlugin::Builtin(plugin) => {
                create_plugin(plugin, working_directory, plugin_name).await
            }
            SupportedPlugin::Mock(plugin) => {
                create_plugin(plugin, working_directory, plugin_name).await
            }
            SupportedPlugin::Mcp(plugin) => {
                create_plugin(plugin, working_directory, plugin_name).await
            }
        };
        let plugin = plugin.attach_printable_lazy(|| format!("plugin: {plugin_name}"))?;
        Ok(plugin)
    }
}
