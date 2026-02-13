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
use stepflow_core::workflow::StepId;
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{
    BlobApiUrl, DynPlugin, Plugin, PluginConfig, PluginError, Result, RunContext,
    StepflowEnvironment,
};
use stepflow_state::blob_ref_ops;
use tokio::sync::RwLock;

use crate::error::TransportError;
use crate::http::{HttpClient, HttpClientHandle};
use crate::protocol::{
    ComponentExecuteParams, ComponentInfoParams, ComponentListParams, InitializeParams,
    Initialized, ObservabilityContext, RuntimeCapabilities,
};
use crate::subprocess::{SubprocessHandle, SubprocessLauncher};

#[derive(Serialize, Deserialize, Debug, utoipa::ToSchema)]
pub struct StepflowPluginConfig {
    #[serde(flatten)]
    pub transport: StepflowTransport,
}

/// Configuration for Stepflow plugin transport.
///
/// Either `command` or `url` must be provided (but not both):
/// - `command`: Launch a subprocess HTTP server
/// - `url`: Connect to an existing HTTP server
#[derive(Serialize, Deserialize, Debug, Clone, utoipa::ToSchema)]
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
        /// Health check configuration for the subprocess server.
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "healthCheck"
        )]
        health_check: Option<HealthCheckConfig>,
    },
    /// Remote mode: connect directly to an existing HTTP endpoint.
    Remote { url: String },
}

/// Configuration for health check polling when launching subprocess servers.
#[derive(Serialize, Deserialize, Debug, Clone, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckConfig {
    /// Health check endpoint path. Default: "/health"
    #[serde(default = "default_health_path")]
    pub path: String,
    /// Total timeout in milliseconds for the health check to pass. Default: 60000 (60s)
    #[serde(default = "default_health_timeout_ms")]
    pub timeout_ms: u64,
    /// Delay between health check attempts in milliseconds. Default: 100
    #[serde(default = "default_health_retry_delay_ms")]
    pub retry_delay_ms: u64,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            path: default_health_path(),
            timeout_ms: default_health_timeout_ms(),
            retry_delay_ms: default_health_retry_delay_ms(),
        }
    }
}

fn default_health_path() -> String {
    "/health".to_string()
}

fn default_health_timeout_ms() -> u64 {
    60000
}

fn default_health_retry_delay_ms() -> u64 {
    100
}

impl PluginConfig for StepflowPluginConfig {
    type Error = TransportError;

    async fn create_plugin(
        self,
        working_directory: &std::path::Path,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        match self.transport {
            StepflowTransport::Subprocess {
                command,
                args,
                env,
                health_check,
            } => {
                let launcher = SubprocessLauncher::try_new(
                    working_directory.to_owned(),
                    command,
                    args,
                    env,
                    health_check,
                )?;

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
    /// Blob API URL to pass to workers during initialization.
    /// Set during ensure_initialized from the environment.
    blob_api_url: RwLock<Option<String>>,
    /// Byte size threshold for automatic blobification of inputs/outputs.
    /// 0 means disabled. Set during ensure_initialized from the environment.
    blob_threshold: RwLock<usize>,
    /// Whether the worker supports `$blob` references in inputs/outputs.
    /// Set from the InitializeResult response.
    supports_blob_refs: RwLock<bool>,
}

impl StepflowPlugin {
    fn new(state: StepflowPluginState) -> Self {
        Self {
            state: RwLock::new(state),
            blob_api_url: RwLock::new(None),
            blob_threshold: RwLock::new(0),
            supports_blob_refs: RwLock::new(false),
        }
    }

    /// Create RuntimeCapabilities from the stored blob API URL and threshold.
    async fn create_capabilities(&self) -> Option<RuntimeCapabilities> {
        let url = self.blob_api_url.read().await.clone();
        let threshold = *self.blob_threshold.read().await;
        let blob_threshold = if threshold > 0 { Some(threshold) } else { None };

        if url.is_some() || blob_threshold.is_some() {
            Some(RuntimeCapabilities {
                blob_api_url: url,
                blob_threshold,
            })
        } else {
            None
        }
    }

    /// Check if blobification should be applied (worker supports it and threshold is set).
    async fn should_blobify(&self) -> bool {
        *self.supports_blob_refs.read().await && *self.blob_threshold.read().await > 0
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
    async fn get_or_create_client_handle(&self) -> Result<HttpClientHandle> {
        // First try to get existing handle
        if let Ok(handle) = self.client_handle().await {
            return Ok(handle);
        }

        // If not initialized, create a new client
        self.create_client().await
    }

    /// Restart the subprocess and reinitialize the client.
    /// Only valid for subprocess mode - returns error for remote mode.
    async fn restart_subprocess(&self) -> Result<HttpClientHandle> {
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
        let client = HttpClient::try_new(url)
            .await
            .change_context(PluginError::Execution)
            .attach_printable("Unable to create HTTP client for restarted subprocess")?;

        // Reinitialize the server
        let observability = ObservabilityContext::from_current_span();
        let handle = client.handle();

        // Reuse capabilities from initial initialization
        let capabilities = self.create_capabilities().await;

        let init_result = handle
            .method(
                &InitializeParams {
                    runtime_protocol_version: 1,
                    observability: if observability.trace_id.is_some() {
                        Some(observability)
                    } else {
                        None
                    },
                    capabilities,
                },
                None, // No run context for initialization (no bidirectional)
            )
            .await
            .change_context(PluginError::Execution)
            .attach_printable("Failed to initialize restarted subprocess")?;

        // Update supports_blob_refs from restarted server
        *self.supports_blob_refs.write().await = init_result.supports_blob_refs;

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

    async fn create_client(&self) -> Result<HttpClientHandle> {
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
                let client = HttpClient::try_new(url)
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
                let client = HttpClient::try_new(url)
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
    async fn ensure_initialized(&self, env: &Arc<StepflowEnvironment>) -> Result<()> {
        // Check if already initialized - this makes the method idempotent
        if self.client_handle().await.is_ok() {
            return Ok(());
        }

        // Store blob API URL and threshold from environment
        if let Some(blob_api_url) = env.get::<BlobApiUrl>() {
            *self.blob_api_url.write().await = blob_api_url.url().map(|s| s.to_string());
            *self.blob_threshold.write().await = blob_api_url.blob_threshold();
        }

        let client = self.create_client().await?;

        // Create observability context for initialization (trace only, no flow/run)
        let observability = ObservabilityContext::from_current_span();

        // Create capabilities with blob API URL and threshold
        let capabilities = self.create_capabilities().await;

        let init_result = client
            .method(
                &InitializeParams {
                    runtime_protocol_version: 1,
                    observability: if observability.trace_id.is_some() {
                        Some(observability)
                    } else {
                        None
                    },
                    capabilities,
                },
                None, // No run context for initialization (no bidirectional)
            )
            .await
            .change_context(PluginError::Initializing)?;

        // Record whether this worker supports blob refs
        *self.supports_blob_refs.write().await = init_result.supports_blob_refs;

        client
            .notify(&Initialized {})
            .await
            .change_context(PluginError::Initializing)?;

        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .method(&ComponentListParams {}, None) // No run context for component listing
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
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
        input: ValueRef,
    ) -> Result<FlowResult> {
        const MAX_ATTEMPTS: u32 = 3;
        let can_restart = self.is_subprocess_mode().await;

        // Blobify large input fields if the worker supports blob refs
        let should_blobify = self.should_blobify().await;
        let execute_input = if should_blobify {
            let threshold = *self.blob_threshold.read().await;
            let blob_store = run_context.blob_store();
            let (blobified, _created_ids) = blob_ref_ops::blobify_inputs(
                input.as_ref().clone(),
                threshold,
                blob_store.as_ref(),
            )
            .await
            .change_context(PluginError::Execution)
            .attach_printable("Failed to blobify input")?;
            ValueRef::new(blobified)
        } else {
            input
        };

        for attempt in 1..=MAX_ATTEMPTS {
            // Create observability context from run context and step
            let observability = ObservabilityContext::from_run_context(run_context, step);

            // Get the client handle (will create if not initialized)
            let client_handle = self.get_or_create_client_handle().await?;

            let result = client_handle
                .method(
                    &ComponentExecuteParams {
                        component: component.clone(),
                        input: execute_input.clone(),
                        attempt,
                        observability,
                    },
                    Some(Arc::clone(run_context)),
                )
                .await;

            match result {
                Ok(response) => {
                    // Resolve any blob refs in the output (safe no-op if none present)
                    let output = if should_blobify {
                        let blob_store = run_context.blob_store();
                        let resolved = blob_ref_ops::resolve_blob_refs(
                            response.output.as_ref().clone(),
                            blob_store.as_ref(),
                        )
                        .await
                        .change_context(PluginError::Execution)
                        .attach_printable("Failed to resolve blob refs in output")?;
                        ValueRef::new(resolved)
                    } else {
                        response.output
                    };
                    return Ok(FlowResult::Success(output));
                }
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
                        if let Err(restart_err) = self.restart_subprocess().await {
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
