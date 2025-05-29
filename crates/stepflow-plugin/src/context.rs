use std::{future::Future, pin::Pin, sync::Arc};
use stepflow_core::{
    FlowResult,
    blob::BlobId,
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

    /// Store JSON data as a blob and return its content-based ID.
    ///
    /// The blob ID is generated as a SHA-256 hash of the JSON content,
    /// providing deterministic IDs and automatic deduplication.
    ///
    /// # Arguments
    /// * `data` - The JSON data to store as a blob
    ///
    /// # Returns
    /// The blob ID for the stored data
    fn create_blob(
        &self,
        data: ValueRef,
    ) -> Pin<Box<dyn Future<Output = Result<BlobId>> + Send + '_>>;

    /// Retrieve JSON data by blob ID.
    ///
    /// # Arguments
    /// * `blob_id` - The blob ID to retrieve
    ///
    /// # Returns
    /// The JSON data associated with the blob ID, or an error if not found
    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> Pin<Box<dyn Future<Output = Result<ValueRef>> + Send + '_>>;
}
