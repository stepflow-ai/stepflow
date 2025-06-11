use std::{collections::HashMap, pin::Pin, sync::Arc};

use crate::workflow_executor::execute_workflow;
use crate::{ExecutionError, Result};
use error_stack::ResultExt as _;
use futures::FutureExt as _;
use stepflow_core::{
    FlowError, FlowResult,
    workflow::{Component, Flow, ValueRef},
};
use stepflow_plugin::{Context, DynPlugin, ExecutionContext, Plugin as _};
use stepflow_state::{InMemoryStateStore, StateStore};
use tokio::sync::{RwLock, oneshot};
use uuid::Uuid;

type FutureFlowResult = futures::future::Shared<oneshot::Receiver<FlowResult>>;

/// Main executor of StepFlow workflows.
pub struct StepFlowExecutor {
    state_store: Arc<dyn StateStore>,
    plugins: RwLock<HashMap<String, Arc<DynPlugin<'static>>>>,
    /// Pending workflows and their result futures.
    // TODO: Should treat this as a cache and evict old executions.
    // TODO: Should write execution state to the state store for persistence.
    pending: Arc<RwLock<HashMap<Uuid, FutureFlowResult>>>,
    // Keep a weak reference to self for spawning tasks without circular references
    self_weak: std::sync::Weak<Self>,
}

impl StepFlowExecutor {
    /// Create a new stepflow executor with a custom state store.
    pub fn new(state_store: Arc<dyn StateStore>) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            plugins: RwLock::new(HashMap::new()),
            state_store,
            pending: Arc::new(RwLock::new(HashMap::new())),
            self_weak: weak.clone(),
        })
    }

    /// Create a new stepflow executor with an in-memory state store.
    ///
    /// Will initialize the plugins.
    pub fn new_in_memory() -> Arc<Self> {
        Self::new(Arc::new(InMemoryStateStore::new()))
    }

    pub fn executor(&self) -> Arc<Self> {
        match self.self_weak.upgrade() {
            Some(arc) => arc,
            None => {
                panic!("Executor has been dropped");
            }
        }
    }

    pub fn execution_context(&self, execution_id: Uuid) -> ExecutionContext {
        ExecutionContext::new(self.executor(), execution_id)
    }

    /// Get a reference to the state store.
    pub fn state_store(&self) -> Arc<dyn StateStore> {
        self.state_store.clone()
    }

    pub async fn get_plugin(&self, component: &Component) -> Result<Arc<DynPlugin<'static>>> {
        let protocol = component.protocol();
        let guard = self.plugins.read().await;
        let plugin = guard
            .get(protocol)
            .cloned()
            .ok_or_else(|| ExecutionError::UnregisteredProtocol(protocol.to_owned()))?;
        Ok(plugin)
    }

    /// List all registered plugins and their protocols
    pub async fn list_plugins(&self) -> Vec<(String, Arc<DynPlugin<'static>>)> {
        let guard = self.plugins.read().await;
        guard.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Register a plugin for the given protocol.
    ///
    /// The plugin should be wrapped in `DynPlugin` first, which can be done using
    /// `DynPlugin::boxed(plugin)`.
    pub async fn register_plugin(
        &self,
        protocol: String,
        plugin: Box<DynPlugin<'static>>,
    ) -> Result<()> {
        let plugin: Arc<DynPlugin<'static>> = Arc::from(plugin);

        // Initialize the plugin
        let context: Arc<dyn Context> = self.executor();
        plugin
            .init(&context)
            .await
            .change_context(ExecutionError::PluginError)?;

        // Add the plugin to the registry
        let mut guard = self.plugins.write().await;
        guard.insert(protocol, plugin);
        Ok(())
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
    /// * `input` - The input value for the workflow
    ///
    /// # Returns
    /// A unique execution ID for the submitted workflow
    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        input: ValueRef,
    ) -> Pin<Box<dyn Future<Output = stepflow_plugin::Result<Uuid>> + Send + '_>> {
        let executor = self.executor();

        Box::pin(async move {
            let execution_id = Uuid::new_v4();
            let (tx, rx) = oneshot::channel();

            // Store the receiver for later retrieval
            {
                let mut pending = self.pending.write().await;
                pending.insert(execution_id, rx.shared());
            }

            // Spawn the execution
            tokio::spawn(async move {
                tracing::info!("Executing workflow using tracker-based execution");
                let state_store = executor.state_store.clone();

                let result =
                    execute_workflow(executor, flow, execution_id, input, state_store).await;

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
        })
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
    fn flow_result(
        &self,
        execution_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = stepflow_plugin::Result<FlowResult>> + Send + '_>> {
        Box::pin(async move {
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
        })
    }

    fn state_store(&self) -> &Arc<dyn StateStore> {
        &self.state_store
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
        let executor = StepFlowExecutor::new(state_store.clone());

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
