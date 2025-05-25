use error_stack::ResultExt;
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{Plugin, PluginError, Result};

use super::ClientHandle;

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
