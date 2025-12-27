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
use stepflow_core::{
    FlowError, FlowResult, GetRunOptions, SubmitRunParams,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{Context, DynPlugin, ExecutionContext, Plugin as _, routing::PluginRouter};
use stepflow_state::{InMemoryStateStore, RunStatus, StateStore};
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
            self_weak: weak.clone(),
        })
    }

    /// Initialize all plugins in the plugin router
    pub async fn initialize_plugins(self: &Arc<Self>) -> Result<()> {
        let context: Arc<dyn Context> = self.clone();
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

    /// Create a debug session for step-by-step execution control
    pub async fn debug_session(&self, run_id: Uuid) -> Result<WorkflowExecutor> {
        // Create a new debug session from state store data
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
        // TODO: Retrieve variables from execution state store
        let input = execution.inputs.first().cloned().unwrap_or_default();
        let mut workflow_executor = WorkflowExecutor::new(
            self.executor(),
            workflow,
            flow_id,
            run_id,
            input,
            self.state_store.clone(),
            None, // Variables not supported in debug sessions yet
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
    /// Submit a run with 1 or N items.
    ///
    /// If `params.wait` is false, returns immediately with status=Running.
    /// If `params.wait` is true, blocks until completion and returns final status.
    fn submit_run(
        &self,
        params: SubmitRunParams,
    ) -> BoxFuture<'_, stepflow_plugin::Result<RunStatus>> {
        let executor = self.executor();
        let wait = params.wait;

        async move {
            let run_id = Uuid::now_v7();
            let state_store = executor.state_store();
            let item_count = params.item_count();
            let max_concurrency = params.max_concurrency.unwrap_or(item_count);

            // Ensure the flow is stored as a blob
            let flow_value = ValueRef::new(serde_json::to_value(params.flow.as_ref()).unwrap());
            self.state_store
                .put_blob(flow_value, stepflow_core::BlobType::Flow)
                .await
                .change_context(stepflow_plugin::PluginError::Execution)?;

            // Create run record with all inputs
            let mut run_params = stepflow_state::CreateRunParams::new(
                run_id,
                params.flow_id.clone(),
                params.inputs.clone(),
            );
            run_params.workflow_name = params.flow.name().map(|s| s.to_string());
            if let Some(ref o) = params.overrides {
                run_params.overrides = o.clone();
            }
            state_store
                .create_run(run_params)
                .await
                .change_context(stepflow_plugin::PluginError::Execution)?;

            // Collect inputs with their indices for execution
            let indexed_inputs: Vec<_> = params.inputs.into_iter().enumerate().collect();

            // Spawn background task for execution
            let executor_clone = executor.clone();
            let flow_clone = params.flow.clone();
            let flow_id_clone = params.flow_id.clone();
            let overrides_clone = params.overrides.clone();

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
                    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));
                    let mut tasks = vec![];
                    let results = Arc::new(tokio::sync::Mutex::new(Vec::with_capacity(item_count)));

                    for (idx, input) in indexed_inputs {
                        let permit = match semaphore.clone().acquire_owned().await {
                            Ok(permit) => permit,
                            Err(_) => {
                                log::error!("Semaphore closed, aborting run execution");
                                break;
                            }
                        };

                        let executor_ref = executor_clone.clone();
                        let flow_ref = flow_clone.clone();
                        let flow_id_ref = flow_id_clone.clone();
                        let overrides_ref = overrides_clone.clone();
                        let run_ctx = SpanContext::current_local_parent();
                        let results_ref = results.clone();

                        let task = tokio::spawn(async move {
                            let _permit = permit;

                            // Use private helper methods for flow execution
                            let flow_result = match executor_ref
                                .submit_flow_item(
                                    flow_ref,
                                    flow_id_ref,
                                    input,
                                    overrides_ref,
                                    run_ctx,
                                )
                                .await
                            {
                                Ok(submitted_run_id) => {
                                    match executor_ref.get_flow_item_result(submitted_run_id).await
                                    {
                                        Ok(result) => result,
                                        Err(e) => {
                                            log::error!(
                                                "Run item {idx} failed to get result: {e:?}"
                                            );
                                            FlowResult::Failed(FlowError::new(
                                                500,
                                                "Failed to get result",
                                            ))
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Run item {idx} failed to submit: {e:?}");
                                    FlowResult::Failed(FlowError::new(500, "Failed to submit"))
                                }
                            };

                            // Record result immediately
                            let _ = executor_ref
                                .state_store()
                                .record_item_result(run_id, idx, flow_result.clone())
                                .await;

                            // Also store in memory for status computation
                            results_ref.lock().await.push((idx, flow_result));
                        });

                        tasks.push(task);
                    }

                    for task in tasks {
                        let _ = task.await;
                    }

                    // Determine final status from collected results
                    let all_results = results.lock().await;
                    let has_failures = all_results
                        .iter()
                        .any(|(_, r)| matches!(r, FlowResult::Failed(_)));
                    let final_status = if has_failures {
                        stepflow_core::status::ExecutionStatus::Failed
                    } else {
                        stepflow_core::status::ExecutionStatus::Completed
                    };

                    // Update the run status (results already recorded individually)
                    let _ = executor_clone
                        .state_store()
                        .update_run_status(run_id, final_status)
                        .await;

                    log::info!("Run {run_id} execution completed");
                }
                .in_span(run_span)
                .await
            });

            // If wait=true, wait for completion and include results
            if wait {
                while let Ok(Some(details)) = state_store.get_run(run_id).await {
                    match details.summary.status {
                        stepflow_core::status::ExecutionStatus::Running
                        | stepflow_core::status::ExecutionStatus::Paused => {
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                        _ => {
                            // Include results when wait=true
                            return Ok(RunStatus::from_details_with_results(&details));
                        }
                    }
                }
            }

            // Return current status without results
            let details = state_store
                .get_run(run_id)
                .await
                .change_context(stepflow_plugin::PluginError::Execution)?
                .ok_or_else(|| {
                    error_stack::report!(stepflow_plugin::PluginError::Execution)
                        .attach_printable(format!("Run not found: {}", run_id))
                })?;

            Ok(RunStatus::from_details(&details))
        }
        .boxed()
    }

    /// Get run status and optionally results.
    fn get_run(
        &self,
        run_id: Uuid,
        options: GetRunOptions,
    ) -> BoxFuture<'_, stepflow_plugin::Result<RunStatus>> {
        async move {
            let state_store = self.state_store();

            // If wait=true, poll until run completes
            if options.wait {
                while let Ok(Some(details)) = state_store.get_run(run_id).await {
                    match details.summary.status {
                        stepflow_core::status::ExecutionStatus::Running
                        | stepflow_core::status::ExecutionStatus::Paused => {
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                        _ => break,
                    }
                }
            }

            // Get run details
            let details = state_store
                .get_run(run_id)
                .await
                .change_context(stepflow_plugin::PluginError::Execution)?
                .ok_or_else(|| {
                    error_stack::report!(stepflow_plugin::PluginError::Execution)
                        .attach_printable(format!("Run not found: {}", run_id))
                })?;

            // Convert to RunStatus with or without results
            if options.include_results {
                // TODO: Handle result_order (ByCompletion) when we have per-item completion tracking
                Ok(RunStatus::from_details_with_results(&details))
            } else {
                Ok(RunStatus::from_details(&details))
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
}

// ============================================================================
// Private helper methods for workflow execution
// ============================================================================

impl StepflowExecutor {
    /// Submits a single workflow item for execution and returns it's execution ID.
    ///
    /// This method starts the workflow execution in the background and immediately
    /// returns a unique ID that can be used to retrieve the result later.
    async fn submit_flow_item(
        &self,
        flow: std::sync::Arc<stepflow_core::workflow::Flow>,
        flow_id: stepflow_core::BlobId,
        input: ValueRef,
        overrides: Option<stepflow_core::workflow::WorkflowOverrides>,
        parent_context: Option<stepflow_observability::fastrace::prelude::SpanContext>,
    ) -> stepflow_plugin::Result<Uuid> {
        let executor = self.executor();
        let run_id = Uuid::now_v7();

        let (tx, rx) = oneshot::channel();

        // Store the receiver for later retrieval
        {
            let mut pending = self.pending.write().await;
            pending.insert(run_id, rx.shared());
        }

        // Spawn the execution
        tokio::spawn(async move {
            use stepflow_observability::fastrace::prelude::*;

            log::info!("Executing workflow using tracker-based execution");
            let state_store = executor.state_store.clone();

            // Create span for this flow execution
            let span_context = parent_context.unwrap_or_else(|| {
                SpanContext::new(TraceId(Uuid::now_v7().as_u128()), SpanId::default())
            });

            let span = Span::root("flow_execution", span_context)
                .with_property(|| ("run_id", run_id.to_string()));

            // Apply overrides if provided
            let final_flow = if let Some(overrides) = &overrides {
                match stepflow_core::workflow::apply_overrides(flow.clone(), overrides) {
                    Ok(modified_flow) => modified_flow,
                    Err(e) => {
                        log::error!("Failed to apply overrides: {e}");
                        let _ = tx.send(FlowResult::Failed(FlowError::new(
                            400,
                            "Failed to apply overrides",
                        )));
                        return;
                    }
                }
            } else {
                flow.clone()
            };

            let result = execute_workflow(
                executor,
                final_flow,
                flow_id,
                run_id,
                input,
                state_store,
                None, // Variables not supported in item execution
            )
            .in_span(span)
            .await;

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

    /// Retrieves the result of a previously submitted workflow item.
    ///
    /// This method will wait for the workflow to complete if it's still running.
    async fn get_flow_item_result(&self, run_id: Uuid) -> stepflow_plugin::Result<FlowResult> {
        // Get the receiver for this execution
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
                            "Flow execution was cancelled",
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
