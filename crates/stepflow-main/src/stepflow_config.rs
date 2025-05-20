use std::path::{Path, PathBuf};

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_plugin::Plugins;
use stepflow_plugin_protocol::stdio::{Client, StdioPlugin};

#[derive(Serialize, Deserialize)]
pub struct StepflowConfig {
    /// Working directory for the configuration.
    ///
    /// If not set, this will be the directory containing the config.
    pub working_directory: Option<PathBuf>,
    pub plugins: Vec<PluginConfig>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginConfig {
    Stdio {
        name: String,
        command: String,
        args: Vec<String>,
    },
}

impl StepflowConfig {
    /// Create the plugins from the config.
    ///
    /// This consumes the entries in `plugins`.
    pub async fn create_plugins(&mut self) -> Result<Plugins> {
        let working_directory = self.working_directory.as_ref().expect("working_directory");

        let mut registry = Plugins::new();
        for plugin in self.plugins.drain(..) {
            plugin.register(working_directory, &mut registry).await?;
        }

        registry
            .initialize()
            .await
            .change_context(MainError::InitializePlugins)?;

        Ok(registry)
    }
}

impl PluginConfig {
    pub async fn register(self, working_directory: &Path, plugins: &mut Plugins) -> Result<()> {
        match self {
            Self::Stdio {
                name,
                command,
                args,
            } => {
                // Look for the command in the system path and the working directory.
                let command = which::WhichConfig::new()
                    .system_path_list()
                    .custom_cwd(working_directory.to_owned())
                    .binary_name(command.clone().into())
                    .first_result()
                    .change_context(MainError::MissingCommand(command))?;

                let client = Client::builder(command)
                    .args(args)
                    .working_directory(working_directory)
                    .build()
                    .await
                    .change_context(MainError::InstantiatePlugin)?;

                let plugin = StdioPlugin::new(client.handle());
                plugins
                    .register(name, plugin)
                    .change_context(MainError::InstantiatePlugin)?;
            }
        };

        Ok(())
    }
}
