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
use stepflow_server::{ServiceOptions, StepflowService};

use crate::{
    MainError, Result,
    args::{config::ConfigArgs, file_loader::load},
};

/// Create executor from StepflowConfig, starting the service.
///
/// The service provides the blob API, gRPC services for pull-based
/// workers, and orchestrator callbacks (sub-run submission, run status).
/// It is already accepting connections when this function returns.
async fn create_environment(config: StepflowConfig) -> Result<Arc<StepflowEnvironment>> {
    let service = StepflowService::new(config, ServiceOptions::default())
        .await
        .change_context(MainError::Configuration)?;

    // The service is running in the background. Dropping StepflowService
    // detaches the server task (it keeps running until process exit).
    // CancellationToken only cancels on explicit .cancel(), not on drop.
    let env = service.environment().clone();
    drop(service);

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

    /// Create a [`StepflowService`] from a config.
    ///
    /// Unlike [`Self::create_executor_from_config`], this keeps the service alive
    /// so the caller can shut it down explicitly via [`StepflowService::abort`].
    /// The environment is accessible via [`StepflowService::environment`].
    /// This ensures proper cleanup of the gRPC server and worker subprocesses.
    pub async fn create_service_from_config(config: StepflowConfig) -> Result<StepflowService> {
        StepflowService::new(config, ServiceOptions::default())
            .await
            .change_context(MainError::Configuration)
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
