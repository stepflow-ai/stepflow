use std::{future::Future, pin::Pin, sync::Arc};
use stepflow_core::{
    FlowResult,
    workflow::{Flow, ValueRef},
};
use uuid::Uuid;

use crate::Result;

/// Context provided to plugins during workflow execution.
///
/// This trait allows plugins to interact with the workflow execution environment,
/// including the ability to execute nested workflows, without creating circular
/// dependencies between crates.
pub trait ExecutionContext: Send + Sync {
    /// Returns the unique execution ID for this context.
    fn execution_id(&self) -> Uuid;

    /// Submits a nested workflow for execution and returns its execution ID.
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
    ) -> Pin<Box<dyn Future<Output = Result<Uuid>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = Result<FlowResult>> + Send + '_>>;

    /// Executes a nested workflow and waits for its completion.
    ///
    /// This is a convenience method that combines `submit_flow` and `flow_result`.
    ///
    /// # Arguments
    /// * `flow` - The workflow to execute
    /// * `input` - The input value for the workflow
    ///
    /// # Returns
    /// The result of executing the nested workflow
    fn execute_flow(
        &self,
        flow: Arc<Flow>,
        input: ValueRef,
    ) -> Pin<Box<dyn Future<Output = Result<FlowResult>> + Send + '_>> {
        Box::pin(async move {
            let execution_id = self.submit_flow(flow, input).await?;
            self.flow_result(execution_id).await
        })
    }
}
