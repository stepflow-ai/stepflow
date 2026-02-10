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

use error_stack::ResultExt as _;
use std::{path::Path, sync::Arc};
use stepflow_config::StepflowConfig;
use stepflow_core::workflow::Flow;
use stepflow_plugin::StepflowEnvironment;
use stepflow_server::AppConfig;

use crate::{
    MainError, Result,
    args::{config::ConfigArgs, file_loader::load},
};

/// Create executor from StepflowConfig, starting a background HTTP server if needed.
///
/// When `blob_api.url` is not configured but `blob_api.enabled` is true,
/// this function starts a background HTTP server to serve the blob API
/// and other orchestrator endpoints.
async fn create_environment(config: StepflowConfig) -> Result<Arc<StepflowEnvironment>> {
    let needs_server = config.blob_api.enabled && config.blob_api.url.is_none();

    // Bind to port 0 to get an available port for the background server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .change_context(MainError::Configuration)
        .attach_printable("Failed to bind HTTP server for blob API")?;

    let port = listener
        .local_addr()
        .change_context(MainError::Configuration)?
        .port();

    // Create the environment (auto-configures blob API URL from listener port)
    let env = stepflow_server::create_environment(config, &listener)
        .await
        .change_context(MainError::Configuration)?;

    if needs_server {
        // Spawn a background HTTP server to serve blob API endpoints
        let app = AppConfig {
            include_swagger: false,
            include_cors: false,
        }
        .create_app_router(env.clone(), port);

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                log::error!("Background HTTP server error: {e}");
            }
        });
    }

    Ok(env)
}

/// Shared workflow operations
pub struct WorkflowLoader;

impl WorkflowLoader {
    /// Load a workflow from a file path
    pub fn load_workflow(path: &Path) -> Result<Arc<Flow>> {
        load(path)
    }

    /// Create an executor from a StepflowConfig
    pub async fn create_executor_from_config(
        config: StepflowConfig,
    ) -> Result<Arc<StepflowEnvironment>> {
        create_environment(config).await
    }

    /// Load config and create executor in one step
    pub async fn load_and_create_executor(
        flow_path: Option<&Path>,
        config_path: Option<std::path::PathBuf>,
    ) -> Result<Arc<StepflowEnvironment>> {
        let config_args = ConfigArgs::with_path(config_path);
        let config = config_args.load_config(flow_path)?;
        create_environment(config).await
    }

    /// Load workflow, config, and create executor - full setup
    pub async fn setup_workflow_execution(
        workflow_path: &Path,
        config_path: Option<std::path::PathBuf>,
    ) -> Result<(Arc<Flow>, Arc<StepflowEnvironment>)> {
        let workflow = Self::load_workflow(workflow_path)?;
        let executor = Self::load_and_create_executor(Some(workflow_path), config_path).await?;
        Ok((workflow, executor))
    }
}
