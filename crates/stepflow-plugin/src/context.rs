use futures::future::{BoxFuture, FutureExt as _};
use std::sync::Arc;
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowResult,
    workflow::{Flow, ValueRef},
};
use stepflow_state::StateStore;
use uuid::Uuid;

/// Trait for interacting with the workflow runtime.
pub trait Context: Send + Sync {
    /// Submits a nested workflow for execution and returns its execution ID.
    ///
    /// Implementation should use Arc::clone on self if it needs to pass ownership
    /// to spawned tasks or other async contexts.
    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        workflow_hash: FlowHash,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<Uuid>>;

    /// Retrieves the result of a previously submitted workflow.
    fn flow_result(&self, execution_id: Uuid) -> BoxFuture<'_, crate::Result<FlowResult>>;

    /// Executes a nested workflow and waits for its completion.
    fn execute_flow(
        &self,
        flow: Arc<Flow>,
        workflow_hash: FlowHash,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<FlowResult>> {
        async move {
            let execution_id = self.submit_flow(flow, workflow_hash, input).await?;
            self.flow_result(execution_id).await
        }
        .boxed()
    }

    /// Get the state store for this executor.
    fn state_store(&self) -> &Arc<dyn StateStore>;
}

/// Execution context that combines a Context with an execution ID.
#[derive(Clone)]
pub struct ExecutionContext {
    context: Arc<dyn Context>,
    execution_id: Uuid,
}

impl ExecutionContext {
    /// Create a new ExecutionContext.
    pub fn new(context: Arc<dyn Context>, execution_id: Uuid) -> Self {
        Self {
            context,
            execution_id,
        }
    }

    /// Get the execution ID for this context.
    pub fn execution_id(&self) -> Uuid {
        self.execution_id
    }

    /// Get a reference to the state store.
    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        self.context.state_store()
    }
}

impl Context for ExecutionContext {
    fn state_store(&self) -> &Arc<dyn StateStore> {
        self.context.state_store()
    }

    /// Submit a nested workflow for execution.
    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        workflow_hash: FlowHash,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<Uuid>> {
        self.context.submit_flow(flow, workflow_hash, input)
    }

    /// Get the result of a workflow execution.
    fn flow_result(&self, execution_id: Uuid) -> BoxFuture<'_, crate::Result<FlowResult>> {
        self.context.flow_result(execution_id)
    }

    /// Execute a nested workflow and wait for completion.
    fn execute_flow(
        &self,
        flow: Arc<Flow>,
        workflow_hash: FlowHash,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<FlowResult>> {
        self.context.execute_flow(flow, workflow_hash, input)
    }
}
