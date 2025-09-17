// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

use std::borrow::Cow;
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
    /// Maximum number of retry attempts for component execution (default: 3)
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_max_retries() -> u32 {
    3
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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
        let max_retries = self.max_retries;

        let transport = self.transport.clone();
        match transport {
            StepflowTransport::Stdio {
                ref command,
                ref args,
                ref env,
            } => {
                let launcher = Launcher::try_new(
                    working_directory.to_owned(),
                    command.clone(),
                    args.clone(),
                    env.clone(),
                )?;

                Ok(DynPlugin::boxed(StepflowPlugin::new(
                    StepflowPluginState::UninitializedStdio(launcher),
                    max_retries,
                    transport,
                    working_directory.to_owned(),
                )))
            }
            StepflowTransport::Http { ref url } => Ok(DynPlugin::boxed(StepflowPlugin::new(
                StepflowPluginState::UninitializedHttp(url.clone()),
                max_retries,
                transport,
                working_directory.to_owned(),
            ))),
        }
    }
}

pub struct StepflowPlugin {
    state: RwLock<StepflowPluginState>,
    max_retries: u32,
    // Keep original config to restore state after clearing client handle
    original_config: StepflowTransport,
    working_directory: std::path::PathBuf,
}

impl StepflowPlugin {
    fn new(
        state: StepflowPluginState,
        max_retries: u32,
        original_config: StepflowTransport,
        working_directory: std::path::PathBuf,
    ) -> Self {
        Self {
            state: RwLock::new(state),
            max_retries,
            original_config,
            working_directory,
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

    /// Clear the current client handle, forcing recreation on next use
    async fn clear_client_handle(&self) {
        let mut guard = self.state.write().await;
        if matches!(*guard, StepflowPluginState::Initialized(_)) {
            // Restore original uninitialized state instead of Empty
            match &self.original_config {
                StepflowTransport::Stdio { command, args, env } => {
                    match Launcher::try_new(
                        self.working_directory.clone(),
                        command.clone(),
                        args.clone(),
                        env.clone(),
                    ) {
                        Ok(launcher) => {
                            *guard = StepflowPluginState::UninitializedStdio(launcher);
                        }
                        Err(e) => {
                            tracing::error!("Failed to recreate launcher during clear: {e:?}");
                            *guard = StepflowPluginState::Empty;
                        }
                    }
                }
                StepflowTransport::Http { url } => {
                    *guard = StepflowPluginState::UninitializedHttp(url.clone());
                }
            }
        }
    }

    /// Get client handle, creating it if necessary
    async fn get_or_create_client_handle(
        &self,
        context: Arc<dyn Context>,
    ) -> Result<StepflowClientHandle> {
        // First try to get existing handle
        if let Ok(handle) = self.client_handle().await {
            return Ok(handle);
        }

        // If not initialized, create a new client
        self.create_client(context).await
    }

    /// Check if an error is a transport error that should trigger process restart and retry
    fn is_transport_error(error: &error_stack::Report<PluginError>) -> bool {
        // Look for TransportError in the error chain
        error.contains::<crate::error::TransportError>()
    }

    /// Wait for restart counter to increase beyond the initial value
    async fn wait_for_restart_count_change(
        &self,
        initial_count: Option<super::stdio::client::RestartCounter>,
        restart_rx: &mut Option<tokio::sync::watch::Receiver<super::stdio::client::RestartCounter>>,
    ) -> Result<()> {
        if let (Some(initial), Some(restart_rx)) = (initial_count, restart_rx) {
            // Check if restart already happened
            let current_count = *restart_rx.borrow();
            if current_count > initial {
                return Ok(());
            }

            // Wait for counter to increase with timeout
            let timeout = std::time::Duration::from_secs(10);
            match tokio::time::timeout(timeout, restart_rx.changed()).await {
                Ok(Ok(())) => {
                    let new_count = *restart_rx.borrow();
                    if new_count > initial {
                        Ok(())
                    } else {
                        // Continue anyway if counter didn't increase
                        Ok(())
                    }
                }
                Ok(Err(_)) => {
                    tracing::warn!("Restart counter channel closed");
                    Err(PluginError::Execution).attach_printable("restart counter channel closed")
                }
                Err(_) => {
                    tracing::warn!("Timeout waiting for process restart");
                    Err(PluginError::Execution).attach_printable("restart timeout")
                }
            }
        } else {
            // HTTP transport doesn't need restart synchronization
            Ok(())
        }
    }

    /// Execute component with a single attempt (extracted from original execute method)
    async fn try_execute_component(
        &self,
        component: &Component,
        context: &ExecutionContext,
        input: &ValueRef,
        attempt: u32,
    ) -> Result<FlowResult> {
        let step_id = context
            .step_id()
            .ok_or_else(|| {
                error_stack::report!(PluginError::Internal(Cow::Borrowed("missing step ID")))
            })?
            .to_owned();

        let run_id = context.run_id();
        let flow_id = context
            .flow_id()
            .ok_or_else(|| {
                error_stack::report!(PluginError::Internal(Cow::Borrowed("missing flow ID")))
            })?
            .clone();

        // Use get_or_create_client_handle to handle reinitialization after restart
        let client_handle = self
            .get_or_create_client_handle(context.context().clone())
            .await?;
        let response = client_handle
            .method(&ComponentExecuteParams {
                component: component.clone(),
                input: input.clone(),
                step_id,
                run_id: run_id.to_string(),
                flow_id,
                attempt,
            })
            .await
            .change_context(PluginError::Execution)?;

        Ok(FlowResult::Success(response.output))
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
        context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let max_attempts = self.max_retries;
        tracing::debug!("Starting component execution with max_retries={max_attempts}");

        for attempt in 1..=max_attempts {
            tracing::debug!("Attempting component execution (attempt {attempt}/{max_attempts})");

            // Get restart counter BEFORE attempting execution
            let (initial_restart_count, mut restart_rx) =
                if let Ok(handle) = self.client_handle().await {
                    match &handle {
                        StepflowClientHandle::Stdio(stdio_handle) => {
                            let (count, rx) = stdio_handle.restart_counter();
                            (Some(count), Some(rx))
                        }
                        StepflowClientHandle::Http(_) => (None, None),
                    }
                } else {
                    (None, None)
                };

            match self
                .try_execute_component(component, &context, &input, attempt)
                .await
            {
                Ok(result) => {
                    if attempt > 1 {
                        tracing::info!(
                            "Component execution succeeded on attempt {attempt}/{max_attempts}"
                        );
                    } else {
                        tracing::debug!("Component execution succeeded on first attempt");
                    }
                    return Ok(result);
                }
                Err(e) if attempt < max_attempts && Self::is_transport_error(&e) => {
                    tracing::warn!(
                        "Component execution failed (attempt {attempt}/{max_attempts}) due to transport error, will retry: {e:?}"
                    );

                    // Clear the invalid client handle so next attempt gets a fresh one
                    self.clear_client_handle().await;

                    // Wait for restart counter to increase (indicating restart completion)
                    self.wait_for_restart_count_change(initial_restart_count, &mut restart_rx)
                        .await?;
                    continue;
                }
                Err(e) => {
                    if Self::is_transport_error(&e) {
                        tracing::error!(
                            "Component execution failed after {max_attempts} attempts (transport error): {e:?}"
                        );
                    } else {
                        tracing::debug!(
                            "Component execution failed (non-transport error, no retry): {e:?}"
                        );
                    }
                    tracing::debug!(
                        "Error chain analysis: contains TransportError={}",
                        e.contains::<crate::error::TransportError>()
                    );
                    return Err(e);
                }
            }
        }

        // This should never be reached due to the loop logic, but satisfy the compiler
        unreachable!("execute loop should have returned or errored")
    }
}
