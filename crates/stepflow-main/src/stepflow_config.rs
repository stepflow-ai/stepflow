use std::path::{Path, PathBuf};

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_builtins::BuiltinPluginConfig;
use stepflow_mock::MockPlugin;
use stepflow_plugin::{DynPlugin, PluginConfig};
use stepflow_protocol::stdio::StdioPluginConfig;

#[derive(Serialize, Deserialize)]
pub struct StepflowConfig {
    /// Working directory for the configuration.
    ///
    /// If not set, this will be the directory containing the config.
    pub working_directory: Option<PathBuf>,
    pub plugins: Vec<SupportedPluginConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
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
) -> Result<Box<DynPlugin<'static>>> {
    plugin
        .create_plugin(working_directory)
        .await
        .change_context(MainError::RegisterPlugin)
}

impl SupportedPluginConfig {
    pub async fn instantiate(
        self,
        working_directory: &Path,
    ) -> Result<(String, Box<DynPlugin<'static>>)> {
        let plugin = match self.plugin {
            SupportedPlugin::Stdio(plugin) => create_plugin(plugin, working_directory).await,
            SupportedPlugin::Builtin(plugin) => create_plugin(plugin, working_directory).await,
            SupportedPlugin::Mock(plugin) => create_plugin(plugin, working_directory).await,
        };
        let plugin = plugin.attach_printable_lazy(|| format!("plugin: {}", self.name))?;
        Ok((self.name, plugin))
    }
}
