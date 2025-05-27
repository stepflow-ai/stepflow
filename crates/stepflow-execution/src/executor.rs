use std::{collections::HashMap, pin::Pin, sync::Arc};

use crate::Result;
use futures::{FutureExt as _, TryFutureExt as _};
use stepflow_core::{
    FlowError, FlowResult,
    workflow::{Flow, ValueRef},
};
use stepflow_plugin::{ExecutionContext, PluginError, Plugins};
use tokio::sync::{RwLock, oneshot};
use uuid::Uuid;

type FutureFlowResult = futures::future::Shared<oneshot::Receiver<FlowResult>>;

#[derive(Clone)]
pub struct StepFlowExecutor {
    pub(crate) plugins: Arc<Plugins>,
    // TODO: Should treat this as a cache and evict old executions.
    // TODO: Should write this to state.
    pending: Arc<RwLock<HashMap<Uuid, FutureFlowResult>>>,
}

impl StepFlowExecutor {
    pub fn new(plugins: Plugins) -> Self {
        Self {
            plugins: Arc::new(plugins),
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
}
