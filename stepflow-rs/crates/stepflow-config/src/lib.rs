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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use error_stack::{Report, ResultExt as _};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use stepflow_builtins::BuiltinPluginConfig;
use stepflow_components_mcp::McpPluginConfig;
use stepflow_plugin::routing::RoutingConfig;
use stepflow_plugin::{DynPlugin, PluginConfig};
use stepflow_protocol::StepflowPluginConfig;
use stepflow_state::{InMemoryStateStore, StateStore};
use stepflow_state_sql::{SqliteStateStore, SqliteStateStoreConfig};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration error")]
    Configuration,
}

type Result<T> = std::result::Result<T, Report<ConfigError>>;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
                    .change_context(ConfigError::Configuration)?;
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
            SupportedPlugin::Mcp(plugin) => create_plugin(plugin, working_directory).await?,
        };
        Ok(plugin)
    }
}

impl StepflowConfig {
    /// Load configuration from a YAML file
    pub async fn load_from_file(path: &Path) -> Result<Self> {
        let contents = tokio::fs::read_to_string(path)
            .await
            .change_context(ConfigError::Configuration)
            .attach_printable_lazy(|| format!("Failed to read config file: {}", path.display()))?;

        let mut config: StepflowConfig = serde_yaml_ng::from_str(&contents)
            .change_context(ConfigError::Configuration)
            .attach_printable("Failed to parse config YAML")?;

        // Set working directory to parent of config file if not specified
        if config.working_directory.is_none() {
            config.working_directory = path.parent().map(|p| p.to_path_buf());
        }

        Ok(config)
    }

    /// Create a StepflowExecutor from this configuration
    pub async fn create_executor(self) -> Result<Arc<stepflow_execution::StepflowExecutor>> {
        use stepflow_execution::StepflowExecutor;
        use stepflow_plugin::routing::PluginRouter;

        // Create state store from configuration
        let state_store = self.state_store.create_state_store().await?;

        let working_directory = self
            .working_directory
            .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

        // Build the plugin router
        log::info!("Routing Config: {:?}", self.routing);
        let mut plugin_router_builder = PluginRouter::builder().with_routing_config(self.routing);

        // Register plugins from IndexMap
        for (plugin_name, plugin_config) in self.plugins {
            let plugin = plugin_config
                .instantiate(&working_directory)
                .await
                .attach_printable_lazy(|| {
                    format!("Failed to instantiate plugin for '{plugin_name}'")
                })?;
            plugin_router_builder = plugin_router_builder.register_plugin(plugin_name, plugin);
        }

        let plugin_router = plugin_router_builder
            .build()
            .change_context(ConfigError::Configuration)?;

        let executor = StepflowExecutor::new(state_store, working_directory, plugin_router);

        // Initialize all plugins
        executor
            .initialize_plugins()
            .await
            .change_context(ConfigError::Configuration)?;

        Ok(executor)
    }
}
