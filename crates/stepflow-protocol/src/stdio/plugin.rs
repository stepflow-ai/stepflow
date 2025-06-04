use std::sync::Arc;

use error_stack::ResultExt as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{
    Context, DynPlugin, ExecutionContext, Plugin, PluginConfig, PluginError, Result,
};
use tokio::sync::RwLock;

use super::{
    StdioError,
    client::{Client, ClientHandle},
    launcher::Launcher,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct StdioPluginConfig {
    pub command: String,
    pub args: Vec<String>,
    /// Environment variables to pass to the sub-process.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,
}

impl PluginConfig for StdioPluginConfig {
    type Error = StdioError;

    async fn create_plugin(
        self,
        working_directory: &std::path::Path,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        // Look for the command in the system path and the working directory.
        let Self { command, args, env } = self;

        let launcher = Launcher::try_new(working_directory.to_owned(), command, args, env)?;

        Ok(DynPlugin::boxed(StdioPlugin(RwLock::new(
            StdioPluginState::Uninitialized(launcher),
        ))))
    }
}

pub struct StdioPlugin(RwLock<StdioPluginState>);

enum StdioPluginState {
    Empty,
    Uninitialized(Launcher),
    Initialized(ClientHandle),
}

impl StdioPlugin {
    async fn client_handle(&self) -> Result<ClientHandle> {
        let guard = self.0.read().await;
        match &*guard {
            StdioPluginState::Initialized(handle) => Ok(handle.clone()),
            _ => Err(PluginError::Execution).attach_printable("stdio client not initialized"),
        }
    }

    async fn create_client(&self, context: Arc<dyn Context>) -> Result<ClientHandle> {
        let mut guard = self.0.write().await;
        match std::mem::replace(&mut *guard, StdioPluginState::Empty) {
            StdioPluginState::Uninitialized(launcher) => {
                let client = Client::try_new(launcher, context)
                    .await
                    .change_context(PluginError::Initializing)
                    .attach_printable("Unable to launch component server")?;
                *guard = StdioPluginState::Initialized(client.handle());
                Ok(client.handle())
            }
            _ => Err(PluginError::Initializing)
                .attach_printable_lazy(|| "Unexpected state".to_string()),
        }
    }
}

impl Plugin for StdioPlugin {
    async fn init(&self, context: &Arc<dyn Context>) -> Result<()> {
        let client = self.create_client(context.clone()).await?;

        client
            .request(&crate::schema::initialization::Request {
                runtime_protocol_version: 1,
            })
            .await
            .change_context(PluginError::Initializing)?;

        client
            .notify(&crate::schema::initialization::Complete {})
            .await
            .change_context(PluginError::Initializing)?;

        Ok(())
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        // TODO: Enrich this? Component not found, etc. based on the protocol error code?
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .request(&crate::schema::component_info::Request {
                component: component.clone(),
            })
            .await
            .change_context(PluginError::ComponentInfo)?;

        Ok(response.info)
    }

    async fn execute(
        &self,
        component: &Component,
        _context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let client_handle = self.client_handle().await?;
        let response = client_handle
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
