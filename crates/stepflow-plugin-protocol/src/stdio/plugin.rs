use error_stack::ResultExt;
use stepflow_plugin::{Plugin, PluginError, Result};
use stepflow_protocol::component_info::ComponentInfo;
use stepflow_workflow::{Component, Value};

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
    fn protocol(&self) -> &'static str {
        "stdio"
    }

    async fn init(&self) -> Result<()> {
        self.client
            .request(&stepflow_protocol::initialization::Request {
                runtime_protocol_version: 1,
            })
            .await
            .change_context(PluginError::Initializing)?;

        self.client
            .notify(&stepflow_protocol::initialization::Complete {})
            .await
            .change_context(PluginError::Initializing)?;

        Ok(())
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        // TODO: Enrich this? Component not found, etc. based on the protocol error code?
        let response = self
            .client
            .request(&stepflow_protocol::component_info::Request {
                component: component.clone(),
            })
            .await
            .change_context(PluginError::ComponentInfo)?;

        Ok(response.info)
    }

    async fn execute(&self, component: &Component, input: Value) -> Result<Value> {
        let response = self
            .client
            .request(&stepflow_protocol::component_execute::Request {
                component: component.clone(),
                input,
            })
            .await
            .change_context(PluginError::Execution)?;

        Ok(response.output)
    }
}
