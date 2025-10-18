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

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::workflow_executor::{WorkflowExecutor, execute_workflow};
use crate::{ExecutionError, Result};
use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use stepflow_core::BlobId;
use stepflow_core::{
    FlowError, FlowResult,
    workflow::{Component, Flow, ValueRef},
};
use stepflow_plugin::{Context, DynPlugin, ExecutionContext, Plugin as _, routing::PluginRouter};
use stepflow_state::{InMemoryStateStore, StateStore};
use tokio::sync::{RwLock, oneshot};
use uuid::Uuid;

type FutureFlowResult = futures::future::Shared<oneshot::Receiver<FlowResult>>;

/// Main executor of Stepflow flows.
pub struct StepflowExecutor {
    state_store: Arc<dyn StateStore>,
    working_directory: PathBuf,
    plugin_router: PluginRouter,
    /// Pending flows and their result futures.
    // TODO: Should treat this as a cache and evict old executions.
    // TODO: Should write execution state to the state store for persistence.
    pending: Arc<RwLock<HashMap<Uuid, FutureFlowResult>>>,
    /// Active debug sessions for step-by-step execution control
    debug_sessions: Arc<RwLock<HashMap<Uuid, WorkflowExecutor>>>,
    // Keep a weak reference to self for spawning tasks without circular references
    self_weak: std::sync::Weak<Self>,
}

impl StepflowExecutor {
    /// Create a new stepflow executor with a custom state store and plugin router.
    pub fn new(
        state_store: Arc<dyn StateStore>,
        working_directory: PathBuf,
        plugin_router: PluginRouter,
    ) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            state_store,
            working_directory,
            plugin_router,
            pending: Arc::new(RwLock::new(HashMap::new())),
            debug_sessions: Arc::new(RwLock::new(HashMap::new())),
            self_weak: weak.clone(),
        })
    }

    /// Initialize all plugins in the plugin router
    pub async fn initialize_plugins(&self) -> Result<()> {
        let context: Arc<dyn Context> = self.executor();

        // Initialize each unique plugin once
        for plugin in self.plugin_router.plugins() {
            plugin
                .init(&context)
                .await
                .change_context(ExecutionError::PluginInitialization)?;
        }

        Ok(())
    }

    /// Create a new stepflow executor with an in-memory state store and empty plugin router.
    pub fn new_in_memory() -> Arc<Self> {
        use stepflow_plugin::routing::PluginRouter;
        let plugin_router = PluginRouter::builder().build().unwrap();
        Self::new(
            Arc::new(InMemoryStateStore::new()),
            PathBuf::from("."),
            plugin_router,
        )
    }

    pub fn executor(&self) -> Arc<Self> {
        match self.self_weak.upgrade() {
            Some(arc) => arc,
            None => {
                panic!("Executor has been dropped");
            }
        }
    }

    pub fn execution_context(&self, run_id: Uuid, step_id: String) -> ExecutionContext {
        ExecutionContext::for_step(self.executor(), run_id, step_id)
    }

    /// Get a reference to the state store.
    pub fn state_store(&self) -> Arc<dyn StateStore> {
        self.state_store.clone()
    }

    pub async fn get_plugin_and_component(
        &self,
        component: &Component,
        input: ValueRef,
    ) -> Result<(&Arc<DynPlugin<'static>>, String)> {
        // Use the integrated plugin router to get the plugin and resolved component name
        self.plugin_router
            .get_plugin_and_component(component.path(), input)
            .change_context(ExecutionError::RouterError)
    }

    /// List all registered plugins
    pub async fn list_plugins(&self) -> Vec<&Arc<DynPlugin<'static>>> {
        self.plugin_router.plugins().collect()
    }

    /// Get the plugin router for accessing routing functionality
    pub fn plugin_router(&self) -> &PluginRouter {
        &self.plugin_router
    }

    /// Get or create a debug session for step-by-step execution control
    pub async fn debug_session(&self, run_id: Uuid) -> Result<WorkflowExecutor> {
        // Check if session already exists
        {
            let sessions = self.debug_sessions.read().await;
            if let Some(_session) = sessions.get(&run_id) {
                // Return a clone of the session (WorkflowExecutor should implement Clone if needed)
                // For now, we'll create a new session each time since WorkflowExecutor is not Clone
            }
        }

        // Session doesn't exist, create a new one from state store data
        let execution = self
            .state_store
            .get_run(run_id)
            .await
            .change_context(ExecutionError::StateError)?
            .ok_or_else(|| error_stack::report!(ExecutionError::ExecutionNotFound(run_id)))?;

        // Extract workflow hash from execution details
        let flow_id = execution.summary.flow_id;

        let workflow = self
            .state_store
            .get_flow(&flow_id)
            .await
            .change_context(ExecutionError::StateError)?
            .ok_or_else(|| {
                error_stack::report!(ExecutionError::WorkflowNotFound(flow_id.clone()))
            })?;

        // Create a new WorkflowExecutor for this debug session
        let mut workflow_executor = WorkflowExecutor::new(
            self.executor(),
            workflow,
            flow_id,
            run_id,
            execution.input,
            self.state_store.clone(),
        )?;

        // Recover state from the state store to ensure consistency
        let corrections_made = workflow_executor.recover_from_state_store().await?;
        if corrections_made > 0 {
            log::info!(
                "Recovery completed for run {}: fixed {} status mismatches",
                run_id,
                corrections_made
            );
        }

        Ok(workflow_executor)
    }
}

impl Context for StepflowExecutor {
    /// Submits a nested workflow for execution and returns it's execution ID.
    ///
    /// This method starts the workflow execution in the background and immediately
    /// returns a unique ID that can be used to retrieve the result later.
    ///
    /// # Arguments
    /// * `flow` - The workflow to execute
    /// * 'flow_id` - ID of the workflow
    /// * `input` - The input value for the workflow
    ///
    /// # Returns
    /// A unique execution ID for the submitted workflow
    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        input: ValueRef,
    ) -> BoxFuture<'_, stepflow_plugin::Result<Uuid>> {
        let executor = self.executor();

        async move {
            let run_id = Uuid::new_v4();
            let (tx, rx) = oneshot::channel();

            // Store the receiver for later retrieval
            {
                let mut pending = self.pending.write().await;
                pending.insert(run_id, rx.shared());
            }

            // Spawn the execution
            tokio::spawn(async move {
                log::info!("Executing workflow using tracker-based execution");
                let state_store = executor.state_store.clone();

                let result =
                    execute_workflow(executor, flow, flow_id, run_id, input, state_store).await;

                let flow_result = match result {
                    Ok(flow_result) => flow_result,
                    Err(e) => {
                        if let Some(error) = e.downcast_ref::<FlowError>().cloned() {
                            FlowResult::Failed(error)
                        } else {
                            log::error!("Flow execution failed: {:?}", e);
                            FlowResult::Failed(stepflow_core::FlowError::from_error_stack(e))
                        }
                    }
                };

                // Send the result back
                let _ = tx.send(flow_result);
            });

            Ok(run_id)
        }
        .boxed()
    }

    /// Retrieves the result of a previously submitted workflow.
    ///
    /// This method will wait for the workflow to complete if it's still running.
    ///
    /// # Arguments
    /// * `run_id` - The run ID returned by `submit_flow`
    ///
    /// # Returns
    /// The result of the workflow execution
    fn flow_result(&self, run_id: Uuid) -> BoxFuture<'_, stepflow_plugin::Result<FlowResult>> {
        async move {
            // Remove and get the receiver for this execution
            let receiver = {
                let pending = self.pending.read().await;
                pending.get(&run_id).cloned()
            };

            match receiver {
                Some(rx) => {
                    match rx.await {
                        Ok(result) => Ok(result),
                        Err(_) => {
                            // The sender was dropped, indicating the execution was cancelled or failed
                            Ok(FlowResult::Failed(stepflow_core::FlowError::new(
                                410,
                                "Nested flow execution was cancelled",
                            )))
                        }
                    }
                }
                None => {
                    // Execution ID not found
                    Ok(FlowResult::Failed(stepflow_core::FlowError::new(
                        404,
                        format!("No run found for ID: {run_id}"),
                    )))
                }
            }
        }
        .boxed()
    }

    fn state_store(&self) -> &Arc<dyn StateStore> {
        &self.state_store
    }

    fn working_directory(&self) -> &std::path::Path {
        &self.working_directory
    }

    /// Submit a batch execution and return the batch ID immediately.
    fn submit_batch(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        inputs: Vec<ValueRef>,
        max_concurrency: Option<usize>,
    ) -> BoxFuture<'_, stepflow_plugin::Result<Uuid>> {
        let executor = self.executor();

        async move {
            let batch_id = Uuid::new_v4();
            let state_store = executor.state_store();

            let total_runs = inputs.len();
            let max_concurrency = max_concurrency.unwrap_or(total_runs);

            // Create batch record
            state_store
                .create_batch(batch_id, flow_id.clone(), flow.name(), total_runs)
                .await
                .change_context(stepflow_plugin::PluginError::Execution)?;

            // Create run records and collect (run_id, input, index) tuples
            let mut run_inputs = Vec::with_capacity(total_runs);
            for (idx, input) in inputs.into_iter().enumerate() {
                let run_id = Uuid::new_v4();

                // Create run record
                state_store
                    .create_run(
                        run_id,
                        flow_id.clone(),
                        flow.name(),
                        None,  // No flow label for batch execution
                        false, // Batch runs are not in debug mode
                        input.clone(),
                    )
                    .await
                    .change_context(stepflow_plugin::PluginError::Execution)?;

                // Link run to batch
                state_store
                    .add_run_to_batch(batch_id, run_id, idx)
                    .await
                    .change_context(stepflow_plugin::PluginError::Execution)?;

                run_inputs.push((run_id, input, idx));
            }

            // Spawn background task for batch execution
            let executor_clone = executor.clone();
            let flow_clone = flow.clone();
            let flow_id_clone = flow_id.clone();

            tokio::spawn(async move {
                let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));
                let mut tasks = vec![];

                for (run_id, input, _idx) in run_inputs {
                    let permit = match semaphore.clone().acquire_owned().await {
                        Ok(permit) => permit,
                        Err(_) => {
                            log::error!("Semaphore closed, aborting batch execution");
                            break;
                        }
                    };
                    let executor_ref = executor_clone.clone();
                    let flow_ref = flow_clone.clone();
                    let flow_id_ref = flow_id_clone.clone();

                    let task = tokio::spawn(async move {
                        let _permit = permit; // Hold permit during execution

                        match executor_ref.submit_flow(flow_ref, flow_id_ref, input).await {
                            Ok(submitted_run_id) => {
                                // Wait for the result
                                match executor_ref.flow_result(submitted_run_id).await {
                                    Ok(flow_result) => {
                                        // Update run status based on result
                                        let state_store = executor_ref.state_store();
                                        let status = match &flow_result {
                                            stepflow_core::FlowResult::Success(_) => {
                                                stepflow_core::status::ExecutionStatus::Completed
                                            }
                                            stepflow_core::FlowResult::Failed(_)
                                            | stepflow_core::FlowResult::Skipped { .. } => {
                                                stepflow_core::status::ExecutionStatus::Failed
                                            }
                                        };
                                        let result_ref = match &flow_result {
                                            stepflow_core::FlowResult::Success(r) => {
                                                Some(r.clone())
                                            }
                                            _ => None,
                                        };
                                        let _ = state_store
                                            .update_run_status(run_id, status, result_ref)
                                            .await;
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "Batch run {run_id} failed to get result: {e:?}"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Batch run {run_id} failed to submit: {e:?}");
                            }
                        }
                    });

                    tasks.push(task);
                }

                // Wait for all tasks to complete
                for task in tasks {
                    let _ = task.await;
                }

                log::info!("Batch {batch_id} execution completed");
            });

            Ok(batch_id)
        }
        .boxed()
    }

    /// Get batch status and optionally results, with optional waiting.
    fn get_batch(
        &self,
        batch_id: Uuid,
        wait: bool,
        include_results: bool,
    ) -> BoxFuture<
        '_,
        stepflow_plugin::Result<(
            stepflow_state::BatchDetails,
            Option<Vec<stepflow_state::BatchOutputInfo>>,
        )>,
    > {
        async move {
            let state_store = self.state_store();

            // If wait=true, poll until completion
            if wait {
                loop {
                    // Get batch metadata to verify batch exists
                    let _metadata = state_store
                        .get_batch(batch_id)
                        .await
                        .change_context(stepflow_plugin::PluginError::Execution)?
                        .ok_or_else(|| {
                            error_stack::report!(stepflow_plugin::PluginError::Execution)
                                .attach_printable(format!("Batch not found: {}", batch_id))
                        })?;

                    // Get batch statistics
                    let statistics = state_store
                        .get_batch_statistics(batch_id)
                        .await
                        .change_context(stepflow_plugin::PluginError::Execution)?;

                    // Check if all runs complete
                    if statistics.running_runs == 0 && statistics.paused_runs == 0 {
                        break;
                    }

                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }

            // Get current batch details
            let metadata = state_store
                .get_batch(batch_id)
                .await
                .change_context(stepflow_plugin::PluginError::Execution)?
                .ok_or_else(|| {
                    error_stack::report!(stepflow_plugin::PluginError::Execution)
                        .attach_printable(format!("Batch not found: {}", batch_id))
                })?;

            let statistics = state_store
                .get_batch_statistics(batch_id)
                .await
                .change_context(stepflow_plugin::PluginError::Execution)?;

            // Calculate completion time if all runs are complete
            let completed_at = if metadata.status == stepflow_state::BatchStatus::Cancelled
                || (statistics.running_runs == 0 && statistics.paused_runs == 0)
            {
                Some(chrono::Utc::now())
            } else {
                None
            };

            let details = stepflow_state::BatchDetails {
                metadata,
                statistics,
                completed_at,
            };

            // Get outputs if requested
            let outputs = if include_results {
                // Get runs for this batch
                let filters = stepflow_state::RunFilters::default();
                let batch_runs = state_store
                    .list_batch_runs(batch_id, &filters)
                    .await
                    .change_context(stepflow_plugin::PluginError::Execution)?;

                // Fetch run details for each to get results
                let mut output_infos = Vec::new();
                for (run_summary, batch_input_index) in batch_runs {
                    let run_details = state_store
                        .get_run(run_summary.run_id)
                        .await
                        .change_context(stepflow_plugin::PluginError::Execution)?;

                    let result = run_details.and_then(|details| details.result);

                    output_infos.push(stepflow_state::BatchOutputInfo {
                        batch_input_index,
                        status: run_summary.status,
                        result,
                    });
                }

                // Sort by batch_input_index to maintain input order
                output_infos.sort_by_key(|o| o.batch_input_index);

                Some(output_infos)
            } else {
                None
            };

            Ok((details, outputs))
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_executor_context_blob_operations() {
        // Create executor with default state store
        let executor = StepflowExecutor::new_in_memory();

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
        // Create executor with custom state store
        let state_store = Arc::new(InMemoryStateStore::new());
        use stepflow_plugin::routing::PluginRouter;
        let plugin_router = PluginRouter::builder().build().unwrap();
        let executor =
            StepflowExecutor::new(state_store.clone(), PathBuf::from("."), plugin_router);

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
