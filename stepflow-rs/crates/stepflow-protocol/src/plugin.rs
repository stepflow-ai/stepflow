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

use crate::error::TransportError;
use crate::http::{HttpClient, HttpClientHandle};
use crate::protocol::{
    ComponentExecuteParams, ComponentInfoParams, ComponentListParams, InitializeParams, Initialized,
};
use crate::stdio::{
    client::{StdioClient, StdioClientHandle},
    launcher::Launcher,
};
use serde::de::DeserializeOwned;

#[derive(Clone)]
enum StepflowClientHandle {
    Stdio(StdioClientHandle),
    Http(HttpClientHandle),
}

impl StepflowClientHandle {
    async fn method<I>(&self, params: &I) -> Result<I::Response>
    where
        I: crate::protocol::ProtocolMethod + serde::Serialize + Send + Sync + std::fmt::Debug,
        I::Response: DeserializeOwned + Send + Sync + 'static,
    {
        match self {
            StepflowClientHandle::Stdio(client) => client
                .method(params)
                .await
                .change_context(PluginError::Execution),
            StepflowClientHandle::Http(client) => client
                .method(params)
                .await
                .change_context(PluginError::Execution),
        }
    }

    async fn notify<I>(&self, params: &I) -> Result<()>
    where
        I: crate::protocol::ProtocolNotification + serde::Serialize + Send + Sync + std::fmt::Debug,
    {
        match self {
            StepflowClientHandle::Stdio(client) => client
                .notify(params)
                .await
                .change_context(PluginError::Execution),
            StepflowClientHandle::Http(client) => client
                .notify(params)
                .await
                .change_context(PluginError::Execution),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StepflowPluginConfig {
    #[serde(flatten)]
    pub transport: StepflowTransport,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "transport")]
pub enum StepflowTransport {
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        args: Vec<String>,
        /// Environment variables to pass to the sub-process.
        /// Values can contain environment variable references like ${HOME} or ${USER:-default}.
        #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
        env: IndexMap<String, String>,
    },
    #[serde(rename = "http")]
    Http { url: String },
}

impl PluginConfig for StepflowPluginConfig {
    type Error = TransportError;

    async fn create_plugin(
        self,
        working_directory: &std::path::Path,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        match self.transport {
            StepflowTransport::Stdio { command, args, env } => {
                let launcher = Launcher::try_new(working_directory.to_owned(), command, args, env)?;

                Ok(DynPlugin::boxed(StepflowPlugin::new(
                    StepflowPluginState::UninitializedStdio(launcher),
                )))
            }
            StepflowTransport::Http { url } => Ok(DynPlugin::boxed(StepflowPlugin::new(
                StepflowPluginState::UninitializedHttp(url),
            ))),
        }
    }
}

pub struct StepflowPlugin {
    state: RwLock<StepflowPluginState>,
}

impl StepflowPlugin {
    fn new(state: StepflowPluginState) -> Self {
        Self {
            state: RwLock::new(state),
        }
    }
}

enum StepflowPluginState {
    Empty,
    UninitializedStdio(Launcher),
    UninitializedHttp(String),
    Initialized(StepflowClientHandle),
}

impl StepflowPlugin {
    async fn client_handle(&self) -> Result<StepflowClientHandle> {
        let guard = self.state.read().await;
        match &*guard {
            StepflowPluginState::Initialized(handle) => Ok(handle.clone()),
            _ => Err(PluginError::Execution).attach_printable("client not initialized"),
        }
    }

    async fn create_client(&self, context: Arc<dyn Context>) -> Result<StepflowClientHandle> {
        let mut guard = self.state.write().await;
        match std::mem::replace(&mut *guard, StepflowPluginState::Empty) {
            StepflowPluginState::UninitializedStdio(launcher) => {
                let client = StdioClient::try_new(launcher, context)
                    .await
                    .change_context(PluginError::Initializing)
                    .attach_printable("Unable to launch component server")?;
                let handle = StepflowClientHandle::Stdio(client.handle());
                *guard = StepflowPluginState::Initialized(handle.clone());
                Ok(handle)
            }
            StepflowPluginState::UninitializedHttp(url) => {
                let client = HttpClient::try_new(url, context)
                    .await
                    .change_context(PluginError::Initializing)
                    .attach_printable("Unable to create HTTP client")?;
                let handle = StepflowClientHandle::Http(client.handle());
                *guard = StepflowPluginState::Initialized(handle.clone());
                Ok(handle)
            }
            _ => Err(PluginError::Initializing)
                .attach_printable_lazy(|| "Unexpected state".to_string()),
        }
    }
}

impl Plugin for StepflowPlugin {
    async fn init(&self, context: &Arc<dyn Context>) -> Result<()> {
        let client = self.create_client(context.clone()).await?;

        client
            .method(&InitializeParams {
                runtime_protocol_version: 1,
            })
            .await
            .change_context(PluginError::Initializing)?;

        client
            .notify(&Initialized {})
            .await
            .change_context(PluginError::Initializing)?;

        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .method(&ComponentListParams {})
            .await
            .change_context(PluginError::ComponentInfo)?;

        Ok(response.components)
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        // TODO: Enrich this? Component not found, etc. based on the protocol error code?
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .method(&ComponentInfoParams {
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
            .method(&ComponentExecuteParams {
                component: component.clone(),
                input,
            })
            .await
            .change_context(PluginError::Execution)?;

        Ok(FlowResult::Success(response.output))
    }
}
