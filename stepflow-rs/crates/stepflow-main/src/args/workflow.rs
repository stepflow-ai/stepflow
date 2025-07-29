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

use error_stack::ResultExt as _;
use std::{path::Path, sync::Arc};
use stepflow_core::workflow::Flow;
use stepflow_execution::StepFlowExecutor;
use stepflow_plugin::routing::PluginRouter;

use crate::{
    MainError, Result,
    args::{config::ConfigArgs, file_loader::load},
    stepflow_config::StepflowConfig,
};

/// Create executor from StepflowConfig
async fn create_executor_impl(config: StepflowConfig) -> Result<Arc<StepFlowExecutor>> {
    // Create state store from configuration
    let state_store = config.state_store.create_state_store().await?;

    let working_directory = config
        .working_directory
        .as_ref()
        .expect("working_directory");

    // Build the plugin router
    tracing::info!("Routing Config: {:?}", config.routing);
    let mut plugin_router_builder = PluginRouter::builder().with_routing_config(config.routing);

    // Register plugins from IndexMap
    for (plugin_name, plugin_config) in config.plugins {
        let plugin = plugin_config
            .instantiate(working_directory)
            .await
            .attach_printable_lazy(|| {
                format!("Failed to instantiate plugin for '{plugin_name}'")
            })?;
        plugin_router_builder = plugin_router_builder.register_plugin(plugin_name, plugin);
    }

    let plugin_router = plugin_router_builder
        .build()
        .change_context(MainError::Configuration)?;

    let executor = StepFlowExecutor::new(state_store, working_directory.clone(), plugin_router);

    // Initialize all plugins
    executor
        .initialize_plugins()
        .await
        .change_context(MainError::Configuration)?;

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
