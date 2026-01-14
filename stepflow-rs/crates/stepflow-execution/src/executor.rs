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

use crate::{ExecutionError, Result};
use error_stack::ResultExt as _;
use stepflow_core::workflow::{Flow, ValueRef};
use stepflow_core::{BlobId, GetRunParams, SubmitRunParams, status::ExecutionStatus};
use stepflow_dtos::RunStatus;
use stepflow_state::CreateRunParams;
use uuid::Uuid;

use stepflow_plugin::StepflowEnvironment;

/// Submit a run for execution.
///
/// This creates a run record and spawns a background task to execute it using `FlowExecutor`.
/// Returns immediately with status=Running. Use `wait_for_completion()` if you need to
/// block until the run finishes.
///
/// # Arguments
/// * `env` - The stepflow environment containing state store and plugins
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
    let state_store = env.state_store();
    let item_count = inputs.len();
    let max_concurrency = params.max_concurrency.unwrap_or(item_count);

    // Ensure the flow is stored as a blob
    let flow_value = ValueRef::new(serde_json::to_value(flow.as_ref()).unwrap());
    state_store
        .put_blob(flow_value, stepflow_core::BlobType::Flow)
        .await
        .change_context(ExecutionError::StateStoreError)?;

    // Create run record before spawning so it's immediately available
    let mut run_params = CreateRunParams::new(run_id, flow_id.clone(), inputs.clone());
    run_params.workflow_name = flow.name().map(|s| s.to_string());
    if !params.overrides.is_empty() {
        run_params.overrides = params.overrides.clone();
    }
    state_store
        .create_run(run_params)
        .await
        .change_context(ExecutionError::StateStoreError)?;

    // Spawn background task for execution using FlowExecutor
    let env_clone = env.clone();
    let flow_clone = flow.clone();
    let flow_id_clone = flow_id.clone();
    let overrides_clone = params.overrides.clone();
    let inputs_clone = inputs.clone();
    let state_store_clone = state_store.clone();

    tokio::spawn(async move {
        use stepflow_observability::fastrace::prelude::*;

        // Create span for run execution
        let run_span_context = params.parent_context.unwrap_or_else(|| {
            SpanContext::new(TraceId(Uuid::now_v7().as_u128()), SpanId::default())
        });

        let run_span = Span::root("run_execution", run_span_context)
            .with_property(|| ("run_id", run_id.to_string()))
            .with_property(|| ("item_count", item_count.to_string()))
            .with_property(|| ("max_concurrency", max_concurrency.to_string()));

        async move {
            // Create FlowExecutor for this run
            let flow_executor_result = crate::flow_executor::FlowExecutorBuilder::new(
                env_clone.clone(),
                flow_clone,
                flow_id_clone,
                state_store_clone.clone(),
            )
            .run_id(run_id)
            .inputs(inputs_clone)
            .max_concurrency(max_concurrency);

            // Apply overrides if not empty
            let flow_executor_result = if !overrides_clone.is_empty() {
                flow_executor_result.overrides(overrides_clone)
            } else {
                flow_executor_result
            };

            // Use depth-first scheduler for now (completes items faster)
            let flow_executor_result =
                flow_executor_result.scheduler(Box::new(crate::DepthFirstScheduler::new()));

            let mut flow_executor = match flow_executor_result.build().await {
                Ok(executor) => executor,
                Err(e) => {
                    log::error!("Failed to build FlowExecutor: {:?}", e);
                    let _ = state_store_clone
                        .update_run_status(run_id, ExecutionStatus::Failed)
                        .await;
                    return;
                }
            };

            // Execute all items (handles result recording and status update internally)
            if let Err(e) = flow_executor.execute_to_completion().await {
                log::error!("FlowExecutor failed: {:?}", e);
                // Status already updated by execute_to_completion on error path
                return;
            }

            log::info!(
                "Run {run_id} execution completed with {} items",
                flow_executor.state().item_count()
            );
        }
        .in_span(run_span)
        .await
    });

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
    env.state_store()
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
    let state_store = env.state_store();

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

    #[tokio::test]
    async fn test_executor_context_blob_operations() {
        // Create executor with default state store
        let executor = StepflowEnvironment::new_in_memory().await.unwrap();

        // Test data
        let test_data = json!({"message": "Hello from executor!", "count": 123});
        let value_ref = ValueRef::new(test_data.clone());

        // Create blob through executor context
        let blob_id = executor
            .state_store()
            .put_blob(value_ref, stepflow_core::BlobType::Data)
            .await
            .unwrap();

        // Retrieve blob through executor context
        let retrieved = executor.state_store().get_blob(&blob_id).await.unwrap();

        // Verify data matches
        assert_eq!(retrieved.data().as_ref(), &test_data);
    }

    #[tokio::test]
    async fn test_executor_with_custom_state_store() {
        use std::path::PathBuf;
        use stepflow_plugin::routing::PluginRouter;
        use stepflow_state::{InMemoryStateStore, StateStore};

        // Create executor with custom state store
        let state_store = Arc::new(InMemoryStateStore::new());
        let plugin_router = PluginRouter::builder().build().unwrap();
        let executor =
            StepflowEnvironment::new(state_store.clone(), PathBuf::from("."), plugin_router)
                .await
                .unwrap();

        // Create blob through executor context
        let test_data = json!({"custom": "state store test"});
        let blob_id = executor
            .state_store()
            .put_blob(
                ValueRef::new(test_data.clone()),
                stepflow_core::BlobType::Data,
            )
            .await
            .unwrap();

        // Verify we can retrieve through the direct state store
        let retrieved_direct = state_store.get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved_direct.data().as_ref(), &test_data);

        // And through the executor context
        let retrieved_executor = executor.state_store().get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved_executor.data().as_ref(), &test_data);
    }
}
