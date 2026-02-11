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

//! Run submission and execution functions.
//!
//! This module provides functions for submitting, waiting for, and querying runs
//! using a [`StepflowEnvironment`]. These functions are used by the server/protocol
//! layer to handle run requests.
//!
//! ## Execution Model
//!
//! - `submit_run()` - Submit a run for background execution, returns immediately
//! - `wait_for_completion()` - Block until a run reaches a terminal state
//! - `get_run()` - Get current run status and optionally results
//!
//! For synchronous execution, call `submit_run()` followed by `wait_for_completion()`.

use std::sync::Arc;

use crate::{ExecutionError, Result, RunState};
use error_stack::ResultExt as _;
use stepflow_core::workflow::{Flow, ValueRef, apply_overrides};
use stepflow_core::{BlobId, GetRunParams, SubmitRunParams, status::ExecutionStatus};
use stepflow_dtos::RunStatus;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    ActiveExecutionsExt as _, BlobStoreExt as _, CreateRunParams, ExecutionJournalExt as _,
    JournalEntry, JournalEvent, LeaseManagerExt as _, MetadataStoreExt as _,
    OrchestratorIdExt as _,
};
use uuid::Uuid;

/// Submit a run for execution.
///
/// This creates a run record and spawns a background task to execute it using `FlowExecutor`.
/// Returns immediately with status=Running. Use `wait_for_completion()` if you need to
/// block until the run finishes.
///
/// # Arguments
/// * `env` - The stepflow environment containing state store, plugins, and active executions tracker
/// * `flow` - The workflow to execute
/// * `flow_id` - The blob ID of the flow
/// * `inputs` - Input values for each item in the run
/// * `params` - Optional execution parameters (use `Default::default()` for defaults)
///
/// # Returns
/// The run ID and initial status (usually Running)
pub async fn submit_run(
    env: &Arc<StepflowEnvironment>,
    flow: Arc<Flow>,
    flow_id: BlobId,
    inputs: Vec<ValueRef>,
    params: SubmitRunParams,
) -> Result<RunStatus> {
    let run_id = Uuid::now_v7();
    let state_store = env.metadata_store();
    let item_count = inputs.len();
    let max_concurrency = params.max_concurrency.unwrap_or(item_count);

    // Ensure the flow is stored as a blob
    let flow_value = ValueRef::new(serde_json::to_value(flow.as_ref()).unwrap());
    let blob_store = env.blob_store();
    blob_store
        .put_blob(flow_value, stepflow_core::BlobType::Flow)
        .await
        .change_context(ExecutionError::StateStoreError)?;

    // Create run record before spawning so it's immediately available
    let mut run_params = CreateRunParams::new(run_id, flow_id.clone(), inputs.clone());
    run_params.workflow_name = flow.name().map(|s| s.to_string());
    if !params.overrides.is_empty() {
        run_params.overrides = params.overrides.clone();
    }

    // Set orchestrator_id on the run record for distributed coordination
    if let Some(orch_id) = env.orchestrator_id() {
        run_params.orchestrator_id = Some(orch_id.as_str().to_string());
    }

    state_store
        .create_run(run_params)
        .await
        .change_context(ExecutionError::StateStoreError)?;

    // Acquire lease in the distributed lease manager (creates etcd key for orphan detection)
    if let Some(orch_id) = env.orchestrator_id()
        && let Err(e) = env
            .lease_manager()
            .acquire_lease(run_id, orch_id.clone())
            .await
    {
        log::warn!("Failed to acquire lease for run {run_id}: {e:?}");
    }

    // Journal: Record run creation and flush to ensure durability before execution.
    // This ensures recovery can always find at least the RunCreated event.
    let journal = env.execution_journal();
    let entry = JournalEntry::new(
        run_id,
        run_id, // For root runs, root_run_id == run_id
        JournalEvent::RunCreated {
            flow_id: flow_id.clone(),
            inputs: inputs.clone(),
            variables: params.variables.clone().unwrap_or_default(),
            parent_run_id: None,
        },
    );
    journal
        .append(entry)
        .await
        .change_context(ExecutionError::JournalError)?;
    journal
        .flush(run_id)
        .await
        .change_context(ExecutionError::JournalError)?;

    // Apply overrides to the flow (returns unchanged if empty)
    let flow_with_overrides = match apply_overrides(flow.clone(), &params.overrides) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Failed to apply overrides: {:?}", e);
            let _ = state_store
                .update_run_status(run_id, ExecutionStatus::Failed)
                .await;
            return Err(e.change_context(ExecutionError::OverrideError));
        }
    };

    // Create RunState for this run
    let variables = params.variables.clone().unwrap_or_default();
    let run_state = RunState::new(
        run_id,
        flow_id.clone(),
        flow_with_overrides,
        inputs.clone(),
        variables,
    );

    // Build FlowExecutor before spawning - handle errors synchronously
    let flow_executor = match crate::FlowExecutorBuilder::new(env.clone(), run_state)
        .max_concurrency(max_concurrency)
        .scheduler(Box::new(crate::DepthFirstScheduler::new()))
        .build()
        .await
    {
        Ok(executor) => executor,
        Err(e) => {
            log::error!("Failed to build FlowExecutor: {:?}", e);
            let _ = state_store
                .update_run_status(run_id, ExecutionStatus::Failed)
                .await;
            return Err(e);
        }
    };

    // Spawn the flow executor and register with active executions.
    flow_executor.spawn(env.active_executions());

    // Return current status (will be Running since we just spawned the task)
    get_run(env, run_id, GetRunParams::default()).await
}

/// Wait for a run to reach a terminal state (Completed, Failed, or Cancelled).
///
/// This method efficiently waits without polling by using a broadcast channel
/// notification system. Returns when the run completes.
///
/// # Arguments
/// * `env` - The stepflow environment
/// * `run_id` - The run ID to wait for
///
/// # Returns
/// Ok(()) when the run completes, or an error if the run is not found
pub async fn wait_for_completion(env: &Arc<StepflowEnvironment>, run_id: Uuid) -> Result<()> {
    env.metadata_store()
        .wait_for_completion(run_id)
        .await
        .change_context(ExecutionError::StateStoreError)
}

/// Get run status and optionally results.
///
/// # Arguments
/// * `env` - The stepflow environment
/// * `run_id` - The run ID to query
/// * `params` - Options controlling what data to include
///
/// # Returns
/// The run status and optionally results
pub async fn get_run(
    env: &Arc<StepflowEnvironment>,
    run_id: Uuid,
    params: GetRunParams,
) -> Result<RunStatus> {
    let state_store = env.metadata_store();

    let details = state_store
        .get_run(run_id)
        .await
        .change_context(ExecutionError::StateStoreError)?
        .ok_or_else(|| {
            error_stack::report!(ExecutionError::RunNotFound)
                .attach_printable(format!("Run not found: {}", run_id))
        })?;

    if params.include_results {
        let items = state_store
            .get_item_results(run_id, params.result_order)
            .await
            .change_context(ExecutionError::StateStoreError)?;
        Ok(RunStatus::from_details_with_items(&details, items))
    } else {
        Ok(RunStatus::from_details(&details))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::workflow::ValueRef;
    use stepflow_plugin::StepflowEnvironmentBuilder;

    #[tokio::test]
    async fn test_executor_context_blob_operations() {
        // Create executor with default state store
        let executor = StepflowEnvironmentBuilder::build_in_memory().await.unwrap();

        // Test data
        let test_data = json!({"message": "Hello from executor!", "count": 123});
        let value_ref = ValueRef::new(test_data.clone());

        // Create blob through executor context
        let blob_id = executor
            .blob_store()
            .put_blob(value_ref, stepflow_core::BlobType::Data)
            .await
            .unwrap();

        // Retrieve blob through executor context
        let retrieved = executor.blob_store().get_blob(&blob_id).await.unwrap();

        // Verify data matches
        assert_eq!(retrieved.data().as_ref(), &test_data);
    }

    #[tokio::test]
    async fn test_executor_with_custom_metadata_store() {
        use std::path::PathBuf;
        use stepflow_plugin::routing::PluginRouter;
        use stepflow_state::{BlobStore, InMemoryStateStore, MetadataStore};

        // Create executor with custom metadata store
        let store = Arc::new(InMemoryStateStore::new());
        let metadata_store: Arc<dyn MetadataStore> = store.clone();
        let blob_store: Arc<dyn BlobStore> = store;
        let plugin_router = PluginRouter::builder().build().unwrap();
        let executor = StepflowEnvironmentBuilder::new()
            .metadata_store(metadata_store)
            .blob_store(blob_store.clone())
            .working_directory(PathBuf::from("."))
            .plugin_router(plugin_router)
            .build()
            .await
            .unwrap();

        // Create blob through executor context
        let test_data = json!({"custom": "state store test"});
        let blob_id = executor
            .blob_store()
            .put_blob(
                ValueRef::new(test_data.clone()),
                stepflow_core::BlobType::Data,
            )
            .await
            .unwrap();

        // Verify we can retrieve through the direct blob store
        let retrieved_direct = blob_store.get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved_direct.data().as_ref(), &test_data);

        // And through the executor context
        let retrieved_executor = executor.blob_store().get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved_executor.data().as_ref(), &test_data);
    }
}
