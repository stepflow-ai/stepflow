use std::sync::Arc;

use bit_set::BitSet;
use futures::future::BoxFuture;
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowResult,
    blob::BlobId,
    workflow::{Component, Flow, ValueRef},
};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::StateError;

/// Write operations for async queuing
#[derive(Debug)]
pub enum StateWriteOperation {
    /// Record the result of a step execution.
    ///
    /// This operation may be queued and batched by the implementation for performance.
    /// Use `flush_pending_writes()` if immediate persistence is required.
    ///
    /// # Fields
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_result` - The step result to store
    RecordStepResult {
        execution_id: Uuid,
        step_result: StepResult,
    },
    /// Update multiple steps to the same status.
    ///
    /// This operation may be queued and batched by the implementation for performance.
    /// Use `flush_pending_writes()` if immediate persistence is required.
    ///
    /// # Fields
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `status` - The new status to apply to all specified steps
    /// * `step_indices` - Vector of step indices to update
    UpdateStepStatuses {
        execution_id: Uuid,
        status: StepStatus,
        step_indices: BitSet,
    },
    /// Flush any pending write operations to persistent storage.
    ///
    /// This operation ensures that all queued write operations are completed before returning.
    /// The `execution_id` parameter is a hint to implementations about which writes to prioritize,
    /// but implementations may choose to flush all pending writes regardless of the hint.
    ///
    /// # Fields
    /// * `execution_id` - Hint about which execution's writes to flush (may be ignored by implementation)
    /// * `completion_notify` - Channel to signal completion of the flush operation
    Flush {
        execution_id: Option<Uuid>,
        completion_notify: oneshot::Sender<Result<(), StateError>>,
    },
}

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
    fn put_blob(&self, data: ValueRef) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>>;

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
    ) -> BoxFuture<'_, error_stack::Result<ValueRef, StateError>>;

    /// Retrieve the result of a step execution by step index.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_idx` - The index of the step within the workflow (0-based)
    ///
    /// # Returns
    /// The execution result if found, or an error if not found
    fn get_step_result(
        &self,
        execution_id: Uuid,
        step_idx: usize,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>>;

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
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepResult>, StateError>>;

    // Workflow Management Methods

    /// Store a workflow by its content hash.
    ///
    /// # Arguments
    /// * `workflow` - The workflow to store
    ///
    /// # Returns
    /// The content hash of the stored workflow
    fn store_workflow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<FlowHash, StateError>>;

    /// Retrieve a workflow by its content hash.
    ///
    /// # Arguments
    /// * `workflow_hash` - The content hash of the workflow
    ///
    /// # Returns
    /// The workflow if found, or an error if not found
    fn get_workflow(
        &self,
        workflow_hash: &FlowHash,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>>;

    /// Get all workflows with a specific name, ordered by creation time (newest first).
    ///
    /// # Arguments
    /// * `name` - The workflow name to search for
    ///
    /// # Returns
    /// A vector of (workflow_hash, created_at) tuples for workflows with the given name
    #[allow(clippy::type_complexity)]
    fn get_workflows_by_name(
        &self,
        name: &str,
    ) -> BoxFuture<
        '_,
        error_stack::Result<Vec<(FlowHash, chrono::DateTime<chrono::Utc>)>, StateError>,
    >;

    /// Get a named workflow, optionally with a specific label.
    ///
    /// This unified method replaces get_latest_workflow_by_name and get_workflow_by_label.
    /// If label is None, returns the latest workflow with the given name.
    /// If label is Some, returns the workflow with that specific label.
    ///
    /// # Arguments
    /// * `name` - The workflow name
    /// * `label` - Optional label name. If None, returns latest workflow.
    ///
    /// # Returns
    /// The workflow with metadata if found
    fn get_named_workflow(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<Option<WorkflowWithMetadata>, StateError>>;

    /// Create or update a workflow label.
    ///
    /// # Arguments
    /// * `name` - The workflow name (from workflow.name field)
    /// * `label` - The label name (like "production", "staging")
    /// * `workflow_hash` - The content hash of the workflow
    ///
    /// # Returns
    /// Success if the label was created/updated
    fn create_or_update_label(
        &self,
        name: &str,
        label: &str,
        workflow_hash: FlowHash,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// List all labels for a specific workflow name.
    ///
    /// # Arguments
    /// * `name` - The workflow name
    ///
    /// # Returns
    /// A vector of workflow labels with metadata
    fn list_labels_for_name(
        &self,
        name: &str,
    ) -> BoxFuture<'_, error_stack::Result<Vec<WorkflowLabelMetadata>, StateError>>;

    /// List all workflow names.
    ///
    /// # Returns
    /// A vector of all unique workflow names in the system
    fn list_workflow_names(&self) -> BoxFuture<'_, error_stack::Result<Vec<String>, StateError>>;

    /// Delete a workflow label.
    ///
    /// # Arguments
    /// * `name` - The workflow name
    /// * `label` - The label name
    ///
    /// # Returns
    /// Success if the label was deleted
    fn delete_label(
        &self,
        name: &str,
        label: &str,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Create a new execution record.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the execution
    /// * `workflow_hash` - Workflow hash
    /// * `workflow_name` - Optional workflow name (from workflow.name field)
    /// * `workflow_label` - Optional workflow label used for execution
    /// * `debug_mode` - Whether execution is in debug mode
    /// * `input` - Input data as JSON
    ///
    /// # Returns
    /// Success if the execution was created
    fn create_execution(
        &self,
        execution_id: Uuid,
        workflow_hash: FlowHash,
        workflow_name: Option<&str>,
        workflow_label: Option<&str>,
        debug_mode: bool,
        input: ValueRef,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Update execution status.
    ///
    /// # Arguments
    /// * `execution_id` - The execution identifier
    /// * `status` - The new status
    /// * `result` - Optional result data as JSON
    ///
    /// # Returns
    /// Success if the execution was updated
    fn update_execution_status(
        &self,
        execution_id: Uuid,
        status: ExecutionStatus,
        result: Option<ValueRef>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Get execution details.
    ///
    /// # Arguments
    /// * `execution_id` - The execution identifier
    ///
    /// # Returns
    /// The execution details if found
    fn get_execution(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<ExecutionDetails>, StateError>>;

    /// List executions with optional filtering.
    ///
    /// # Arguments
    /// * `filters` - Optional filters for the query
    ///
    /// # Returns
    /// A vector of execution summaries
    fn list_executions(
        &self,
        filters: &ExecutionFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<ExecutionSummary>, StateError>>;

    /// Flush any pending write operations to persistent storage.
    ///
    /// This method ensures that all queued write operations are completed before returning.
    /// The `execution_id` parameter is a hint to implementations about which writes to prioritize,
    /// but implementations may choose to flush all pending writes regardless of the hint.
    ///
    /// # Arguments
    /// * `execution_id` - Hint about which execution's writes to flush (may be ignored by implementation)
    ///
    /// # Returns
    /// Success when all relevant pending writes have been persisted
    fn flush_pending_writes(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Queue a write operation for async processing.
    ///
    /// This provides a generic interface for queueing write operations.
    /// Implementations can choose whether to process operations immediately
    /// (like InMemoryStateStore) or queue them for batching (like SqliteStateStore).
    ///
    /// # Arguments
    /// * `operation` - The write operation to queue
    ///
    /// # Returns
    /// Success if the operation was queued successfully
    fn queue_write(&self, operation: StateWriteOperation) -> error_stack::Result<(), StateError>;

    // Step Status Management

    /// Initialize step info for an execution.
    fn initialize_step_info(
        &self,
        execution_id: Uuid,
        steps: &[StepInfo],
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Update the status of a single step.
    ///
    /// This operation may be queued and batched by the implementation for performance.
    /// Use `flush_pending_writes()` if immediate persistence is required.
    /// For updating multiple steps efficiently, use `update_step_statuses()`.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_index` - The index of the step to update
    /// * `status` - The new status for the step
    fn update_step_status(
        &self,
        execution_id: Uuid,
        step_index: usize,
        status: stepflow_core::status::StepStatus,
    );

    /// Get all step info for an execution.
    fn get_step_info_for_execution(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>>;

    /// Get runnable steps for an execution based on current status.
    /// Note: This method returns steps based on persistent status only.
    /// Dependency checking should be done by the caller using workflow analysis.
    fn get_runnable_steps(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>>;
}

/// The step result.
#[derive(PartialEq, Debug, Clone)]
pub struct StepResult {
    step_idx: usize,
    step_id: String,
    result: FlowResult,
}

impl StepResult {
    /// Create a new step result.
    pub fn new(step_idx: usize, step_id: impl Into<String>, result: FlowResult) -> Self {
        Self {
            step_idx,
            step_id: step_id.into(),
            result,
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

impl PartialOrd for StepResult {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.step_idx.partial_cmp(&other.step_idx)
    }
}

/// A workflow with its metadata (creation time, label info, etc.)
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowWithMetadata {
    /// The workflow definition
    pub workflow: Arc<Flow>,
    /// The workflow hash
    pub workflow_hash: FlowHash,
    /// When this workflow was created/stored
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional label information if accessed via label
    pub label_info: Option<WorkflowLabelMetadata>,
}

/// Metadata about a workflow label (without the workflow itself)
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowLabelMetadata {
    pub name: String,
    pub label: String,
    pub workflow_hash: FlowHash,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Summary information about a workflow execution.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionSummary {
    pub execution_id: Uuid,
    pub workflow_hash: FlowHash,
    pub workflow_name: Option<String>,
    pub workflow_label: Option<String>,
    pub status: ExecutionStatus,
    pub debug_mode: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Detailed execution information including input and result.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionDetails {
    #[serde(flatten)]
    pub summary: ExecutionSummary,
    pub input: ValueRef,
    pub result: Option<ValueRef>,
}

/// Filters for listing executions.
#[derive(Debug, Clone, Default)]
pub struct ExecutionFilters {
    pub status: Option<ExecutionStatus>,
    pub workflow_name: Option<String>,
    pub workflow_label: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Execution details with resolved input and result blobs.
#[derive(Debug)]
pub struct ExecutionWithBlobs {
    pub execution: ExecutionDetails,
    pub input: Option<ValueRef>,
    pub result: Option<ValueRef>,
}

/// Comprehensive execution step details for server inspection.
#[derive(Debug)]
pub struct ExecutionStepDetails {
    pub execution: ExecutionDetails,
    pub workflow: Option<Arc<stepflow_core::workflow::Flow>>,
    pub step_results: Vec<StepResult>,
    pub input: Option<ValueRef>,
}

/// Complete data needed for debug session creation.
#[derive(Debug)]
pub struct DebugSessionData {
    pub execution: ExecutionDetails,
    pub workflow: Arc<stepflow_core::workflow::Flow>,
    pub input: ValueRef,
    pub step_results: Vec<StepResult>,
}

/// Step information for a workflow execution.
#[derive(Debug, Clone, PartialEq)]
pub struct StepInfo {
    /// Execution ID this step belongs to
    pub execution_id: Uuid,
    /// Index of the step in the workflow
    pub step_index: usize,
    /// Step ID
    pub step_id: String,
    /// Component name/URL
    pub component: Component,
    /// Current status of the step
    pub status: stepflow_core::status::StepStatus,
    /// When the step was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the step was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::status::ExecutionStatus;
    use stepflow_core::workflow::FlowHash;
    use uuid::Uuid;

    #[test]
    fn test_execution_details_serde_flatten() {
        let now = chrono::Utc::now();
        let execution_id = Uuid::new_v4();
        let workflow_hash = FlowHash::from("test-hash");

        let details = ExecutionDetails {
            summary: ExecutionSummary {
                execution_id,
                workflow_hash,
                workflow_name: Some("test-workflow".to_string()),
                workflow_label: Some("production".to_string()),
                status: ExecutionStatus::Completed,
                debug_mode: false,
                created_at: now,
                completed_at: Some(now),
            },
            input: stepflow_core::workflow::ValueRef::new(json!({"test": "input"})),
            result: Some(stepflow_core::workflow::ValueRef::new(
                json!({"test": "output"}),
            )),
        };

        // Serialize the ExecutionDetails
        let serialized = serde_json::to_string(&details).unwrap();

        // Parse as a generic JSON value to verify flattening
        let value: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        // Verify that summary fields are flattened to the top level
        assert_eq!(value["execution_id"], json!(execution_id));
        assert_eq!(value["workflow_name"], json!("test-workflow"));
        assert_eq!(value["workflow_label"], json!("production"));
        assert_eq!(value["status"], json!("completed"));
        assert_eq!(value["debug_mode"], json!(false));

        // Verify that detail-specific fields are also present
        assert_eq!(value["input"], json!({"test": "input"}));
        assert_eq!(value["result"], json!({"test": "output"}));

        // Verify there's no nested "summary" object
        assert!(value.get("summary").is_none());

        // Verify it deserializes back correctly
        let deserialized: ExecutionDetails = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, details);
    }
}
