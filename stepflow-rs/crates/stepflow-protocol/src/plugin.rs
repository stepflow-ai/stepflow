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
    Context, DynPlugin, ExecutionContext, Plugin, PluginConfig, PluginError, Result, RunContext,
};
use tokio::sync::RwLock;

use crate::error::TransportError;
use crate::http::{HttpClient, HttpClientHandle};
use crate::protocol::{
    ComponentExecuteParams, ComponentInfoParams, ComponentListParams, InitializeParams,
    Initialized, ObservabilityContext,
};
use crate::subprocess::{SubprocessHandle, SubprocessLauncher};

#[derive(Serialize, Deserialize, Debug)]
pub struct StepflowPluginConfig {
    #[serde(flatten)]
    pub transport: StepflowTransport,
}

/// Configuration for Stepflow plugin transport.
///
/// Either `command` or `url` must be provided (but not both):
/// - `command`: Launch a subprocess HTTP server
/// - `url`: Connect to an existing HTTP server
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum StepflowTransport {
    /// Subprocess mode: launch a process that runs an HTTP server.
    /// The process must print {"port": N} to stdout when ready.
    Subprocess {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        /// Environment variables to pass to the subprocess.
        /// Values can contain environment variable references like ${HOME} or ${USER:-default}.
        #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
        env: IndexMap<String, String>,
    },
    /// Remote mode: connect directly to an existing HTTP endpoint.
    Remote { url: String },
}

impl PluginConfig for StepflowPluginConfig {
    type Error = TransportError;

    async fn create_plugin(
        self,
        working_directory: &std::path::Path,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        match self.transport {
            StepflowTransport::Subprocess { command, args, env } => {
                let launcher =
                    SubprocessLauncher::try_new(working_directory.to_owned(), command, args, env)?;

                Ok(DynPlugin::boxed(StepflowPlugin::new(
                    StepflowPluginState::UninitializedSubprocess(launcher),
                )))
            }
            StepflowTransport::Remote { url } => Ok(DynPlugin::boxed(StepflowPlugin::new(
                StepflowPluginState::UninitializedRemote(url),
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
    UninitializedSubprocess(SubprocessLauncher),
    UninitializedRemote(String),
    /// Initialized subprocess mode: keeps launcher for restart capability
    InitializedSubprocess {
        launcher: SubprocessLauncher,
        #[allow(dead_code)]
        subprocess: Box<SubprocessHandle>,
        client: HttpClientHandle,
    },
    /// Initialized remote mode: just the client handle
    InitializedRemote(HttpClientHandle),
}

impl StepflowPlugin {
    async fn client_handle(&self) -> Result<HttpClientHandle> {
        let guard = self.state.read().await;
        match &*guard {
            StepflowPluginState::InitializedSubprocess { client, .. } => Ok(client.clone()),
            StepflowPluginState::InitializedRemote(handle) => Ok(handle.clone()),
            _ => Err(PluginError::Execution).attach_printable("client not initialized"),
        }
    }

    /// Check if this plugin is in subprocess mode (can be restarted)
    async fn is_subprocess_mode(&self) -> bool {
        let guard = self.state.read().await;
        matches!(
            &*guard,
            StepflowPluginState::UninitializedSubprocess(_)
                | StepflowPluginState::InitializedSubprocess { .. }
        )
    }

    /// Get client handle, creating it if necessary
    async fn get_or_create_client_handle(
        &self,
        context: Arc<dyn Context>,
    ) -> Result<HttpClientHandle> {
        // First try to get existing handle
        if let Ok(handle) = self.client_handle().await {
            return Ok(handle);
        }

        // If not initialized, create a new client
        self.create_client(context).await
    }

    /// Restart the subprocess and reinitialize the client.
    /// Only valid for subprocess mode - returns error for remote mode.
    async fn restart_subprocess(&self, context: Arc<dyn Context>) -> Result<HttpClientHandle> {
        let mut guard = self.state.write().await;

        // Extract the launcher from current state
        let launcher = match std::mem::replace(&mut *guard, StepflowPluginState::Empty) {
            StepflowPluginState::InitializedSubprocess { launcher, .. } => launcher,
            StepflowPluginState::UninitializedSubprocess(launcher) => launcher,
            other => {
                // Put the state back and return error
                *guard = other;
                return Err(PluginError::Execution)
                    .attach_printable("Cannot restart: not in subprocess mode");
            }
        };

        log::info!("Restarting subprocess HTTP server after crash");

        // Launch new subprocess (clone launcher to keep for future restarts)
        let subprocess = launcher
            .clone()
            .launch()
            .await
            .change_context(PluginError::Execution)
            .attach_printable("Unable to restart subprocess HTTP server")?;

        let url = subprocess.url().to_string();

        // Create new HTTP client
        let client = HttpClient::try_new(url, Arc::downgrade(&context))
            .await
            .change_context(PluginError::Execution)
            .attach_printable("Unable to create HTTP client for restarted subprocess")?;

        // Reinitialize the server
        let observability = ObservabilityContext::from_current_span();
        let handle = client.handle();

        handle
            .method(
                &InitializeParams {
                    runtime_protocol_version: 1,
                    observability: if observability.trace_id.is_some() {
                        Some(observability)
                    } else {
                        None
                    },
                },
                None, // No run context for initialization
            )
            .await
            .change_context(PluginError::Execution)
            .attach_printable("Failed to initialize restarted subprocess")?;

        handle
            .notify(&Initialized {})
            .await
            .change_context(PluginError::Execution)
            .attach_printable("Failed to send initialized notification to restarted subprocess")?;

        // Store new state
        *guard = StepflowPluginState::InitializedSubprocess {
            launcher,
            subprocess: Box::new(subprocess),
            client: handle.clone(),
        };

        Ok(handle)
    }

    async fn create_client(&self, context: Arc<dyn Context>) -> Result<HttpClientHandle> {
        let mut guard = self.state.write().await;
        match std::mem::replace(&mut *guard, StepflowPluginState::Empty) {
            StepflowPluginState::UninitializedSubprocess(launcher) => {
                // Launch subprocess and get URL (clone launcher to keep for restarts)
                let subprocess = launcher
                    .clone()
                    .launch()
                    .await
                    .change_context(PluginError::Initializing)
                    .attach_printable("Unable to launch subprocess HTTP server")?;

                let url = subprocess.url().to_string();

                // Create HTTP client with the subprocess URL
                // Use Weak reference to break circular dependency (executor -> plugin -> client -> context -> executor)
                let client = HttpClient::try_new(url, Arc::downgrade(&context))
                    .await
                    .change_context(PluginError::Initializing)
                    .attach_printable("Unable to create HTTP client for subprocess")?;

                let handle = client.handle();
                // Store launcher for restart capability, subprocess to keep it alive
                *guard = StepflowPluginState::InitializedSubprocess {
                    launcher,
                    subprocess: Box::new(subprocess),
                    client: handle.clone(),
                };
                Ok(handle)
            }
            StepflowPluginState::UninitializedRemote(url) => {
                let client = HttpClient::try_new(url, Arc::downgrade(&context))
                    .await
                    .change_context(PluginError::Initializing)
                    .attach_printable("Unable to create HTTP client")?;

                let handle = client.handle();
                *guard = StepflowPluginState::InitializedRemote(handle.clone());
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

        // Create observability context for initialization (trace only, no flow/run)
        let observability = ObservabilityContext::from_current_span();

        client
            .method(
                &InitializeParams {
                    runtime_protocol_version: 1,
                    observability: if observability.trace_id.is_some() {
                        Some(observability)
                    } else {
                        None
                    },
                },
                None, // No run context for initialization
            )
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
            .method(
                &ComponentListParams {},
                None, // No run context for component listing
            )
            .await
            .change_context(PluginError::ComponentInfo)?;

        Ok(response.components)
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .method(
                &ComponentInfoParams {
                    component: component.clone(),
                },
                None, // No run context for component info
            )
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
        const MAX_ATTEMPTS: u32 = 3;
        let can_restart = self.is_subprocess_mode().await;

        // Create run context for bidirectional handlers
        // For now, root_run_id equals run_id (until sub-flow hierarchy is implemented)
        let run_context = Arc::new(RunContext::for_root(context.run_id()));

        for attempt in 1..=MAX_ATTEMPTS {
            // Create observability context from execution context
            let observability = ObservabilityContext::from_execution_context(&context);

            // Get the client handle (will create if not initialized)
            let client_handle = self
                .get_or_create_client_handle(context.context().clone())
                .await?;

            let result = client_handle
                .method(
                    &ComponentExecuteParams {
                        component: component.clone(),
                        input: input.clone(),
                        attempt,
                        observability,
                    },
                    Some(run_context.clone()),
                )
                .await;

            match result {
                Ok(response) => return Ok(FlowResult::Success(response.output)),
                Err(err) => {
                    // If we can restart (subprocess mode) and have attempts left, try to restart
                    if can_restart && attempt < MAX_ATTEMPTS {
                        log::warn!(
                            "Component execution failed (attempt {}/{}), restarting subprocess: {}",
                            attempt,
                            MAX_ATTEMPTS,
                            err
                        );

                        // Try to restart the subprocess
                        if let Err(restart_err) =
                            self.restart_subprocess(context.context().clone()).await
                        {
                            log::error!("Failed to restart subprocess: {}", restart_err);
                            // If restart fails, return the original error
                            return Err(err).change_context(PluginError::Execution);
                        }

                        // Continue to next attempt
                        continue;
                    }

                    // No more retries or can't restart - return error
                    return Err(err).change_context(PluginError::Execution);
                }
            }
        }

        // Should not reach here, but just in case
        Err(PluginError::Execution).attach_printable("Max retry attempts exceeded")
    }
}
