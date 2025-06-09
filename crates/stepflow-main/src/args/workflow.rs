use error_stack::ResultExt as _;
use std::{path::Path, sync::Arc};
use stepflow_core::workflow::Flow;
use stepflow_execution::StepFlowExecutor;

use crate::{
    MainError, Result,
    args::{config::ConfigArgs, file_loader::load},
    stepflow_config::StepflowConfig,
};

/// Create executor from StepflowConfig
async fn create_executor_impl(config: StepflowConfig) -> Result<Arc<StepFlowExecutor>> {
    // TODO: Allow other state backends.
    let executor = StepFlowExecutor::new_in_memory();

    let working_directory = config
        .working_directory
        .as_ref()
        .expect("working_directory");

    for plugin_config in config.plugins {
        let (protocol, plugin) = plugin_config.instantiate(working_directory).await?;
        executor
            .register_plugin(protocol, plugin)
            .await
            .change_context(MainError::RegisterPlugin)?;
    }
    Ok(executor)
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
    ) -> Result<Arc<StepFlowExecutor>> {
        create_executor_impl(config).await
    }

    /// Load config and create executor in one step
    pub async fn load_and_create_executor(
        flow_path: Option<&Path>,
        config_path: Option<std::path::PathBuf>,
    ) -> Result<Arc<StepFlowExecutor>> {
        let config_args = ConfigArgs::with_path(config_path);
        let config = config_args.load_config(flow_path)?;
        create_executor_impl(config).await
    }

    /// Load workflow, config, and create executor - full setup
    pub async fn setup_workflow_execution(
        workflow_path: &Path,
        config_path: Option<std::path::PathBuf>,
    ) -> Result<(Arc<Flow>, Arc<StepFlowExecutor>)> {
        let workflow = Self::load_workflow(workflow_path)?;
        let executor = Self::load_and_create_executor(Some(workflow_path), config_path).await?;
        Ok((workflow, executor))
    }
}
