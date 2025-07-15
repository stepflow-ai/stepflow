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
use uuid::Uuid;

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
            StepflowClientHandle::Http(client) => {
                // Implement HTTP method calls using the same JSON-RPC approach
                let method = I::METHOD_NAME;
                let params_json =
                    serde_json::to_string(params).change_context(PluginError::Execution)?;
                let id = Uuid::new_v4().to_string();

                // Create JSON-RPC request
                let request = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": method,
                    "params": serde_json::from_str::<serde_json::Value>(&params_json).change_context(PluginError::Execution)?,
                    "id": id
                });

                // Send the request and get response
                let response = client
                    .client()
                    .post(client.url())
                    .json(&request)
                    .send()
                    .await
                    .change_context(PluginError::Execution)?;

                if !response.status().is_success() {
                    return Err(PluginError::Execution.into());
                }

                let response_json: serde_json::Value = response
                    .json()
                    .await
                    .change_context(PluginError::Execution)?;

                // Parse JSON-RPC response
                if let Some(_error) = response_json.get("error") {
                    return Err(PluginError::Execution.into());
                }

                if let Some(result) = response_json.get("result") {
                    serde_json::from_value(result.clone()).change_context(PluginError::Execution)
                } else {
                    Err(PluginError::Execution.into())
                }
            }
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
            StepflowClientHandle::Http(client) => {
                // Implement HTTP notifications using JSON-RPC notification format
                let method = I::METHOD_NAME;
                let params_json =
                    serde_json::to_string(params).change_context(PluginError::Execution)?;

                // Create JSON-RPC notification (no id field)
                let notification = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": method,
                    "params": serde_json::from_str::<serde_json::Value>(&params_json).change_context(PluginError::Execution)?
                });

                // Send the notification (fire and forget)
                client
                    .client()
                    .post(client.url())
                    .json(&notification)
                    .send()
                    .await
                    .change_context(PluginError::Execution)?;

                Ok(())
            }
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
        protocol_prefix: &str,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        match self.transport {
            StepflowTransport::Stdio { command, args, env } => {
                let launcher = Launcher::try_new(working_directory.to_owned(), command, args, env)?;

                Ok(DynPlugin::boxed(StepflowPlugin::new(
                    StepflowPluginState::UninitializedStdio(launcher),
                    protocol_prefix.to_string(),
                )))
            }
            StepflowTransport::Http { url } => Ok(DynPlugin::boxed(StepflowPlugin::new(
                StepflowPluginState::UninitializedHttp(url),
                protocol_prefix.to_string(),
            ))),
        }
    }
}

pub struct StepflowPlugin {
    state: RwLock<StepflowPluginState>,
    protocol_prefix: String,
}

impl StepflowPlugin {
    fn new(state: StepflowPluginState, protocol_prefix: String) -> Self {
        Self {
            state: RwLock::new(state),
            protocol_prefix,
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
                protocol_prefix: self.protocol_prefix.clone(),
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

        Ok(FlowResult::Success {
            result: response.output,
        })
    }
}
