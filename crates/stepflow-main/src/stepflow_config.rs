use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_builtins::BuiltinPluginConfig;
use stepflow_mock::MockPlugin;
use stepflow_plugin::{DynPlugin, PluginConfig};
use stepflow_protocol::stdio::StdioPluginConfig;
use stepflow_state::{InMemoryStateStore, StateStore};
use stepflow_state_sql::{SqliteStateStore, SqliteStateStoreConfig};

#[derive(Serialize, Deserialize)]
pub struct StepflowConfig {
    /// Working directory for the configuration.
    ///
    /// If not set, this will be the directory containing the config.
    pub working_directory: Option<PathBuf>,
    pub plugins: Vec<SupportedPluginConfig>,
    /// State store configuration. If not specified, uses in-memory storage.
    #[serde(default)]
    pub state_store: StateStoreConfig,
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
    Stdio(StdioPluginConfig),
    Builtin(BuiltinPluginConfig),
    Mock(MockPlugin),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SupportedPluginConfig {
    name: String,
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
    ) -> Result<(String, Box<DynPlugin<'static>>)> {
        let plugin = match self.plugin {
            SupportedPlugin::Stdio(plugin) => {
                create_plugin(plugin, working_directory, &self.name).await
            }
            SupportedPlugin::Builtin(plugin) => {
                create_plugin(plugin, working_directory, &self.name).await
            }
            SupportedPlugin::Mock(plugin) => {
                create_plugin(plugin, working_directory, &self.name).await
            }
        };
        let plugin = plugin.attach_printable_lazy(|| format!("plugin: {}", self.name))?;
        Ok((self.name, plugin))
    }
}
