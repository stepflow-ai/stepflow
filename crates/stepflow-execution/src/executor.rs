use std::{collections::HashMap, pin::Pin, sync::Arc};

use crate::Result;
use futures::{FutureExt as _, TryFutureExt as _};
use stepflow_core::{
    FlowError, FlowResult,
    workflow::{Flow, ValueRef},
};
use stepflow_plugin::{ExecutionContext, PluginError, Plugins};
use stepflow_state::{InMemoryStateStore, StateStore};
use tokio::sync::{RwLock, oneshot};
use uuid::Uuid;

type FutureFlowResult = futures::future::Shared<oneshot::Receiver<FlowResult>>;

#[derive(Clone)]
pub struct StepFlowExecutor {
    pub(crate) plugins: Arc<Plugins>,
    pub(crate) state_store: Arc<dyn StateStore>,
    // TODO: Should treat this as a cache and evict old executions.
    // TODO: Should write execution state to the state store for persistence.
    pending: Arc<RwLock<HashMap<Uuid, FutureFlowResult>>>,
}

impl StepFlowExecutor {
    pub fn new(plugins: Plugins) -> Self {
        Self::with_state_store(plugins, Arc::new(InMemoryStateStore::new()))
    }

    pub fn with_state_store(plugins: Plugins, state_store: Arc<dyn StateStore>) -> Self {
        Self {
            plugins: Arc::new(plugins),
            state_store,
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Submits a nested workflow for execution and returns it's execution ID.
    ///
    /// This method starts the workflow execution in the background and immediately
    /// returns a unique ID that can be used to retrieve the result later.
    ///
    /// # Arguments
    /// * `flow` - The workflow to execute
    /// * `input` - The input value for the workflow
    ///
    /// # Returns
    /// A unique execution ID for the submitted workflow
    pub async fn submit_flow(&self, flow: Arc<Flow>, input: ValueRef) -> Result<Uuid> {
        let execution_id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();

        // Store the receiver for later retrieval
        {
            let mut pending = self.pending.write().await;
            pending.insert(execution_id, rx.shared());
        }

        // Clone needed values for the spawned task
        let executor = self.clone();

        // Spawn the execution
        tokio::spawn(async move {
            let result =
                crate::spawn_workflow::spawn_workflow(executor, execution_id, flow, input).await;
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
                                format!("Flow execution failed: {}", e),
                            ),
                        }
                    }
                }
            };

            // Send the result back
            let _ = tx.send(flow_result);
        });

        Ok(execution_id)
    }

    /// Retrieves the result of a previously submitted workflow.
    ///
    /// This method will wait for the workflow to complete if it's still running.
    ///
    /// # Arguments
    /// * `execution_id` - The execution ID returned by `submit_flow`
    ///
    /// # Returns
    /// The result of the workflow execution
    pub async fn flow_result(&self, execution_id: Uuid) -> Result<FlowResult> {
        // Remove and get the receiver for this execution
        let receiver = {
            let pending = self.pending.read().await;
            pending.get(&execution_id).cloned()
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
                        format!("No execution found for ID: {}", execution_id),
                    ),
                })
            }
        }
    }
}

/// Handle for a running workflow.
pub struct ExecutionHandle {
    pub(crate) execution_id: Uuid,
    pub(crate) executor: StepFlowExecutor,
}

impl ExecutionContext for ExecutionHandle {
    fn execution_id(&self) -> Uuid {
        self.execution_id
    }

    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        input: ValueRef,
    ) -> Pin<Box<dyn Future<Output = stepflow_plugin::Result<Uuid>> + Send + '_>> {
        self.executor
            .submit_flow(flow, input)
            .map_err(|e| e.change_context(PluginError::UdfExecution))
            .boxed()
    }

    fn flow_result(
        &self,
        execution_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = stepflow_plugin::Result<FlowResult>> + Send + '_>> {
        self.executor
            .flow_result(execution_id)
            .map_err(|e| e.change_context(PluginError::UdfExecution))
            .boxed()
    }

    fn state_store(&self) -> &Arc<dyn StateStore> {
        &self.executor.state_store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_plugin::Plugins;

    #[tokio::test]
    async fn test_execution_handle_blob_operations() {
        // Create executor with default state store
        let plugins = Plugins::new();
        let executor = StepFlowExecutor::new(plugins);

        // Create execution handle
        let handle = ExecutionHandle {
            execution_id: Uuid::new_v4(),
            executor,
        };

        // Test data
        let test_data = json!({"message": "Hello from execution handle!", "count": 123});
        let value_ref = ValueRef::new(test_data.clone());

        // Create blob through execution context
        let blob_id = handle.state_store().put_blob(value_ref).await.unwrap();

        // Retrieve blob through execution context
        let retrieved = handle.state_store().get_blob(&blob_id).await.unwrap();

        // Verify data matches
        assert_eq!(retrieved.as_ref(), &test_data);
    }

    #[tokio::test]
    async fn test_executor_with_custom_state_store() {
        // Create executor with custom state store
        let plugins = Plugins::new();
        let state_store = Arc::new(InMemoryStateStore::new());
        let executor = StepFlowExecutor::with_state_store(plugins, state_store.clone());

        // Create execution handle
        let handle = ExecutionHandle {
            execution_id: Uuid::new_v4(),
            executor,
        };

        // Create blob through execution context
        let test_data = json!({"custom": "state store test"});
        let blob_id = handle
            .state_store()
            .put_blob(ValueRef::new(test_data.clone()))
            .await
            .unwrap();

        // Verify we can retrieve through the direct state store
        let retrieved_direct = state_store.get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved_direct.as_ref(), &test_data);

        // And through the execution handle
        let retrieved_handle = handle.state_store().get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved_handle.as_ref(), &test_data);
    }
}
