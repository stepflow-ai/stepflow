use std::borrow::Cow;
use std::{future::Future, pin::Pin};

use stepflow_core::{FlowResult, blob::BlobId, workflow::ValueRef};
use uuid::Uuid;

use crate::StateError;

/// Trait for storing and retrieving state data including blobs.
///
/// This trait provides the foundation for both blob storage and future
/// execution state persistence (journaling, checkpointing, etc.).
pub trait StateStore: Send + Sync {
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
    fn put_blob(
        &self,
        data: ValueRef,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<BlobId, StateError>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<ValueRef, StateError>> + Send + '_>>;

    /// Record the result of a step execution.
    ///
    /// This method stores the execution result for a specific step within a workflow
    /// execution, enabling workflow recovery and debugging capabilities.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_idx` - The index of the step within the workflow (0-based)
    /// * `step_id` - The identifier of the step within the workflow
    /// * `result` - The execution result to store
    ///
    /// # Returns
    /// Success if the result was stored, or an error if storage failed
    fn record_step_result(
        &self,
        execution_id: Uuid,
        step_result: StepResult<'_>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>>;

    /// Retrieve the result of a step execution by step index.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_idx` - The index of the step within the workflow (0-based)
    ///
    /// # Returns
    /// The execution result if found, or an error if not found
    fn get_step_result_by_index(
        &self,
        execution_id: Uuid,
        step_idx: usize,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<FlowResult, StateError>> + Send + '_>>;

    /// Retrieve the result of a step execution by step ID.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_id` - The identifier of the step within the workflow
    ///
    /// # Returns
    /// The execution result if found, or an error if not found
    fn get_step_result_by_id(
        &self,
        execution_id: Uuid,
        step_id: &str,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<FlowResult, StateError>> + Send + '_>>;

    /// List all step results for a workflow execution, ordered by step index.
    ///
    /// This is useful for workflow recovery and debugging to see which steps
    /// have completed and their results in workflow order.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    ///
    /// # Returns
    /// A vector of (step_index, step_id, result) tuples, ordered by step_index
    fn list_step_results(
        &self,
        execution_id: Uuid,
    ) -> Pin<
        Box<
            dyn Future<Output = error_stack::Result<Vec<StepResult<'static>>, StateError>>
                + Send
                + '_,
        >,
    >;
}

/// The step result.
#[derive(PartialEq, Debug)]
pub struct StepResult<'a> {
    step_idx: usize,
    step_id: Cow<'a, str>,
    result: FlowResult,
}

impl<'a> StepResult<'a> {
    /// Create a new step result.
    pub fn new(step_idx: usize, step_id: impl Into<Cow<'a, str>>, result: FlowResult) -> Self {
        Self {
            step_idx,
            step_id: step_id.into(),
            result,
        }
    }

    /// Convert to an owned version of the step result.
    pub fn to_owned(&self) -> StepResult<'static> {
        StepResult {
            step_idx: self.step_idx,
            step_id: Cow::Owned(self.step_id.to_string()),
            result: self.result.clone(),
        }
    }

    /// Get the step index.
    pub fn step_idx(&self) -> usize {
        self.step_idx
    }

    /// Get the step ID.
    pub fn step_id(&self) -> &str {
        &self.step_id
    }

    /// Get the step result.
    pub fn result(&self) -> &FlowResult {
        &self.result
    }

    /// Consume self and return the step result.
    pub fn into_result(self) -> FlowResult {
        self.result
    }
}

impl PartialOrd for StepResult<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.step_idx.partial_cmp(&other.step_idx)
    }
}
