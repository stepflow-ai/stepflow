use error_stack::ResultExt;
use serde::{Deserialize, Serialize};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{Plugin, PluginConfig, PluginError, Result};

use super::{
    StdioError,
    client::{Client, ClientHandle},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct StdioPluginConfig {
    pub command: String,
    pub args: Vec<String>,
}

impl PluginConfig for StdioPluginConfig {
    type Plugin = StdioPlugin;
    type Error = StdioError;

    async fn create_plugin(
        self,
        working_directory: &std::path::Path,
    ) -> error_stack::Result<Self::Plugin, Self::Error> {
        // Look for the command in the system path and the working directory.
        let Self { command, args } = self;

        let command = which::WhichConfig::new()
            .system_path_list()
            .custom_cwd(working_directory.to_owned())
            .binary_name(command.clone().into())
            .first_result()
            .change_context_lazy(|| StdioError::MissingCommand(command))?;

        let client = Client::try_new(command, args.clone(), working_directory.to_owned()).await?;

        Ok(StdioPlugin::new(client.handle()))
    }
}

pub struct StdioPlugin {
    client: ClientHandle,
}

impl StdioPlugin {
    pub fn new(client: ClientHandle) -> Self {
        Self { client }
    }
}

impl Plugin for StdioPlugin {
    async fn init(&self) -> Result<()> {
        self.client
            .request(&crate::schema::initialization::Request {
                runtime_protocol_version: 1,
            })
            .await
            .change_context(PluginError::Initializing)?;

        self.client
            .notify(&crate::schema::initialization::Complete {})
            .await
            .change_context(PluginError::Initializing)?;

        Ok(())
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        // TODO: Enrich this? Component not found, etc. based on the protocol error code?
        let response = self
            .client
            .request(&crate::schema::component_info::Request {
                component: component.clone(),
            })
            .await
            .change_context(PluginError::ComponentInfo)?;

        Ok(response.info)
    }

    async fn execute(&self, component: &Component, input: ValueRef) -> Result<FlowResult> {
        let response = self
            .client
            .request(&crate::schema::component_execute::Request {
                component: component.clone(),
                input,
            })
            .await
            .change_context(PluginError::Execution)?;

        Ok(FlowResult::Success {
            result: response.output,
        })
    }
}
