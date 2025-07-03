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
        protocol_prefix: &str,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        // Look for the command in the system path and the working directory.
        let Self { command, args, env } = self;

        let launcher = Launcher::try_new(working_directory.to_owned(), command, args, env)?;

        Ok(DynPlugin::boxed(StdioPlugin::new(
            launcher,
            protocol_prefix.to_string(),
        )))
    }
}

pub struct StdioPlugin {
    state: RwLock<StdioPluginState>,
    protocol_prefix: String,
}

impl StdioPlugin {
    pub fn new(launcher: Launcher, protocol_prefix: String) -> Self {
        Self {
            state: RwLock::new(StdioPluginState::Uninitialized(launcher)),
            protocol_prefix,
        }
    }
}

enum StdioPluginState {
    Empty,
    Uninitialized(Launcher),
    Initialized(ClientHandle),
}

impl StdioPlugin {
    async fn client_handle(&self) -> Result<ClientHandle> {
        let guard = self.state.read().await;
        match &*guard {
            StdioPluginState::Initialized(handle) => Ok(handle.clone()),
            _ => Err(PluginError::Execution).attach_printable("stdio client not initialized"),
        }
    }

    async fn create_client(&self, context: Arc<dyn Context>) -> Result<ClientHandle> {
        let mut guard = self.state.write().await;
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
            .method(&crate::protocol::InitializeParams {
                runtime_protocol_version: 1,
                protocol_prefix: self.protocol_prefix.clone(),
            })
            .await
            .change_context(PluginError::Initializing)?;

        client
            .notify(&crate::protocol::Initialized {})
            .await
            .change_context(PluginError::Initializing)?;

        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .method(&crate::protocol::ComponentListParams {})
            .await
            .change_context(PluginError::ComponentInfo)?;

        Ok(response.components)
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        // TODO: Enrich this? Component not found, etc. based on the protocol error code?
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .method(&crate::protocol::ComponentInfoParams {
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
            .method(&crate::protocol::ComponentExecuteParams {
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
