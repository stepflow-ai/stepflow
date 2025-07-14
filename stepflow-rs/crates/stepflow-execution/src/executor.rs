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

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::workflow_executor::{WorkflowExecutor, execute_workflow};
use crate::{ExecutionError, Result};
use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowError, FlowResult,
    workflow::{Component, Flow, ValueRef},
};
use stepflow_plugin::{Context, DynPlugin, ExecutionContext, Plugin as _, routing::PluginRouter};
use stepflow_state::{InMemoryStateStore, StateStore};
use tokio::sync::{RwLock, oneshot};
use uuid::Uuid;

type FutureFlowResult = futures::future::Shared<oneshot::Receiver<FlowResult>>;

/// Main executor of StepFlow workflows.
pub struct StepFlowExecutor {
    state_store: Arc<dyn StateStore>,
    working_directory: PathBuf,
    plugin_router: PluginRouter,
    /// Pending workflows and their result futures.
    // TODO: Should treat this as a cache and evict old executions.
    // TODO: Should write execution state to the state store for persistence.
    pending: Arc<RwLock<HashMap<Uuid, FutureFlowResult>>>,
    /// Active debug sessions for step-by-step execution control
    debug_sessions: Arc<RwLock<HashMap<Uuid, WorkflowExecutor>>>,
    // Keep a weak reference to self for spawning tasks without circular references
    self_weak: std::sync::Weak<Self>,
}

impl StepFlowExecutor {
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

    pub fn execution_context(&self, run_id: Uuid) -> ExecutionContext {
        ExecutionContext::new(self.executor(), run_id)
    }

    /// Get a reference to the state store.
    pub fn state_store(&self) -> Arc<dyn StateStore> {
        self.state_store.clone()
    }

    pub async fn get_plugin(
        &self,
        component: &Component,
        input: ValueRef,
    ) -> Result<&Arc<DynPlugin<'static>>> {
        // Use the integrated plugin router to get the plugin directly
        self.plugin_router
            .get_plugin(component.path_string(), input)
            .change_context(ExecutionError::RouterError)
    }

    /// List all registered plugins
    pub async fn list_plugins(&self) -> Vec<&Arc<DynPlugin<'static>>> {
        self.plugin_router.plugins().collect()
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
        let flow_hash = execution.summary.flow_hash;

        let workflow = self
            .state_store
            .get_workflow(&flow_hash)
            .await
            .change_context(ExecutionError::StateError)?
            .ok_or_else(|| {
                error_stack::report!(ExecutionError::WorkflowNotFound(flow_hash.clone()))
            })?;

        // Create a new WorkflowExecutor for this debug session
        let mut workflow_executor = WorkflowExecutor::new(
            self.executor(),
            workflow,
            flow_hash,
            run_id,
            execution.input,
            self.state_store.clone(),
        )?;

        // Recover state from the state store to ensure consistency
        let corrections_made = workflow_executor.recover_from_state_store().await?;
        if corrections_made > 0 {
            tracing::info!(
                "Recovery completed for run {}: fixed {} status mismatches",
                run_id,
                corrections_made
            );
        }

        Ok(workflow_executor)
    }
}

impl Context for StepFlowExecutor {
    /// Submits a nested workflow for execution and returns it's execution ID.
    ///
    /// This method starts the workflow execution in the background and immediately
    /// returns a unique ID that can be used to retrieve the result later.
    ///
    /// # Arguments
    /// * `flow` - The workflow to execute
    /// * 'flow_hash` - Hash of the workflow
    /// * `input` - The input value for the workflow
    ///
    /// # Returns
    /// A unique execution ID for the submitted workflow
    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        flow_hash: FlowHash,
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
                tracing::info!("Executing workflow using tracker-based execution");
                let state_store = executor.state_store.clone();

                let result =
                    execute_workflow(executor, flow, flow_hash, run_id, input, state_store).await;

                let flow_result = match result {
                    Ok(flow_result) => flow_result,
                    Err(e) => {
                        if let Some(error) = e.downcast_ref::<FlowError>().cloned() {
                            FlowResult::Failed { error }
                        } else {
                            tracing::error!(?e, "Flow execution failed");
                            FlowResult::Failed {
                                error: stepflow_core::FlowError::new(
                                    500,
                                    format!("Flow execution failed: {e}"),
                                ),
                            }
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
                            Ok(FlowResult::Failed {
                                error: stepflow_core::FlowError::new(
                                    410,
                                    "Nested flow execution was cancelled",
                                ),
                            })
                        }
                    }
                }
                None => {
                    // Execution ID not found
                    Ok(FlowResult::Failed {
                        error: stepflow_core::FlowError::new(
                            404,
                            format!("No run found for ID: {run_id}"),
                        ),
                    })
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_executor_context_blob_operations() {
        // Create executor with default state store
        let executor = StepFlowExecutor::new_in_memory();

        // Test data
        let test_data = json!({"message": "Hello from executor!", "count": 123});
        let value_ref = ValueRef::new(test_data.clone());

        // Create blob through executor context
        let blob_id = executor.state_store().put_blob(value_ref).await.unwrap();

        // Retrieve blob through executor context
        let retrieved = executor.state_store().get_blob(&blob_id).await.unwrap();

        // Verify data matches
        assert_eq!(retrieved.as_ref(), &test_data);
    }

    #[tokio::test]
    async fn test_executor_with_custom_state_store() {
        // Create executor with custom state store
        let state_store = Arc::new(InMemoryStateStore::new());
        use stepflow_plugin::routing::PluginRouter;
        let plugin_router = PluginRouter::builder().build().unwrap();
        let executor =
            StepFlowExecutor::new(state_store.clone(), PathBuf::from("."), plugin_router);

        // Create blob through executor context
        let test_data = json!({"custom": "state store test"});
        let blob_id = executor
            .state_store()
            .put_blob(ValueRef::new(test_data.clone()))
            .await
            .unwrap();

        // Verify we can retrieve through the direct state store
        let retrieved_direct = state_store.get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved_direct.as_ref(), &test_data);

        // And through the executor context
        let retrieved_executor = executor.state_store().get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved_executor.as_ref(), &test_data);
    }
}
