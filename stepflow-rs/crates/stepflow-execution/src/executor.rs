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

use std::path::PathBuf;
use std::sync::Arc;

use crate::{ExecutionError, Result};
use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use stepflow_core::{
    GetRunOptions, SubmitRunParams,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{Context, DynPlugin, Plugin as _, routing::PluginRouter};
use stepflow_state::{InMemoryStateStore, ResultOrder, RunStatus, StateStore};
use uuid::Uuid;

/// Main executor of Stepflow flows.
pub struct StepflowExecutor {
    state_store: Arc<dyn StateStore>,
    working_directory: PathBuf,
    plugin_router: PluginRouter,
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

            // Create run record before spawning so it's immediately available
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

            // Spawn background task for execution using FlowExecutor
            let executor_clone = executor.clone();
            let flow_clone = params.flow.clone();
            let flow_id_clone = params.flow_id.clone();
            let overrides_clone = params.overrides.clone();
            let inputs_clone = params.inputs.clone();
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
                        executor_clone.clone(),
                        flow_clone,
                        flow_id_clone,
                        state_store_clone.clone(),
                    )
                    .run_id(run_id)
                    .inputs(inputs_clone)
                    .max_concurrency(max_concurrency);

                    // Apply overrides if provided
                    let flow_executor_result = if let Some(overrides) = overrides_clone {
                        flow_executor_result.overrides(overrides)
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
                                .update_run_status(
                                    run_id,
                                    stepflow_core::status::ExecutionStatus::Failed,
                                )
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

            // If wait=true, wait for completion and include results
            if wait {
                while let Ok(Some(details)) = state_store.get_run(run_id).await {
                    match details.summary.status {
                        stepflow_core::status::ExecutionStatus::Running
                        | stepflow_core::status::ExecutionStatus::Paused => {
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                        _ => {
                            // Fetch item results separately and include them
                            let items = state_store
                                .get_item_results(run_id, ResultOrder::ByIndex)
                                .await
                                .change_context(stepflow_plugin::PluginError::Execution)?;
                            return Ok(RunStatus::from_details_with_items(&details, items));
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
                // Fetch item results with requested ordering
                let order = match options.result_order {
                    stepflow_core::ResultOrder::ByIndex => ResultOrder::ByIndex,
                    stepflow_core::ResultOrder::ByCompletion => ResultOrder::ByCompletion,
                };
                let items = state_store
                    .get_item_results(run_id, order)
                    .await
                    .change_context(stepflow_plugin::PluginError::Execution)?;
                Ok(RunStatus::from_details_with_items(&details, items))
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
