use std::path::{Path, PathBuf};

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_builtins::BuiltinPluginConfig;
use stepflow_mock::MockPlugin;
use stepflow_plugin::{PluginConfig, Plugins};
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

async fn create_and_register_plugin<P: PluginConfig>(
    protocol: String,
    plugin: P,
    working_directory: &Path,
    registry: &mut Plugins,
) -> Result<()>
where
    P::Plugin: 'static,
{
    let plugin = plugin
        .create_plugin(working_directory)
        .await
        .change_context(MainError::InstantiatePlugin)?;
    registry
        .register(protocol, plugin)
        .change_context(MainError::InstantiatePlugin)?;
    Ok(())
}

impl SupportedPlugin {
    pub async fn register(
        self,
        protocol: String,
        working_directory: &Path,
        registry: &mut Plugins,
    ) -> Result<()> {
        match self {
            Self::Stdio(plugin) => {
                create_and_register_plugin(protocol, plugin, working_directory, registry).await
            }
            Self::Builtin(plugin) => {
                create_and_register_plugin(protocol, plugin, working_directory, registry).await
            }
            Self::Mock(plugin) => {
                create_and_register_plugin(protocol, plugin, working_directory, registry).await
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SupportedPluginConfig {
    name: String,
    #[serde(flatten)]
    plugin: SupportedPlugin,
}

impl StepflowConfig {
    /// Create the plugins from the config.
    ///
    /// This consumes the entries in `plugins`.
    pub async fn create_plugins(self) -> Result<Plugins> {
        let working_directory = self.working_directory.as_ref().expect("working_directory");

        let mut registry = Plugins::new();
        for SupportedPluginConfig { name, plugin } in self.plugins {
            plugin
                .register(name, working_directory, &mut registry)
                .await?;
        }

        registry
            .initialize()
            .await
            .change_context(MainError::InitializePlugins)?;

        Ok(registry)
    }
}
