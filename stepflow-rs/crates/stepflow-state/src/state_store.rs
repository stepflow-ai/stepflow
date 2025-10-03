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

use std::sync::Arc;

use bit_set::BitSet;
use futures::future::{BoxFuture, FutureExt as _};
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{
    BlobData, BlobId, BlobType, FlowResult,
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
    /// * `run_id` - The unique identifier for the workflow execution
    /// * `step_result` - The step result to store
    RecordStepResult {
        run_id: Uuid,
        step_result: StepResult,
    },
    /// Update multiple steps to the same status.
    ///
    /// This operation may be queued and batched by the implementation for performance.
    /// Use `flush_pending_writes()` if immediate persistence is required.
    ///
    /// # Fields
    /// * `run_id` - The unique identifier for the workflow execution
    /// * `status` - The new status to apply to all specified steps
    /// * `step_indices` - Vector of step indices to update
    UpdateStepStatuses {
        run_id: Uuid,
        status: StepStatus,
        step_indices: BitSet,
    },
    /// Flush any pending write operations to persistent storage.
    ///
    /// This operation ensures that all queued write operations are completed before returning.
    /// The `run_id` parameter is a hint to implementations about which writes to prioritize,
    /// but implementations may choose to flush all pending writes regardless of the hint.
    ///
    /// # Fields
    /// * `run_id` - Hint about which run's writes to flush (may be ignored by implementation)
    /// * `completion_notify` - Channel to signal completion of the flush operation
    Flush {
        run_id: Option<Uuid>,
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
    /// * `blob_type` - The type of blob being stored
    ///
    /// # Returns
    /// The blob ID for the stored data
    fn put_blob(
        &self,
        data: ValueRef,
        blob_type: BlobType,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>>;

    /// Retrieve blob data with type information by blob ID.
    ///
    /// # Arguments
    /// * `blob_id` - The blob ID to retrieve
    ///
    /// # Returns
    /// The blob data with type information, or an error if not found
    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<BlobData, StateError>>;

    /// Retrieve blob data only if it matches the expected type.
    ///
    /// # Arguments
    /// * `blob_id` - The blob ID to retrieve
    /// * `expected_type` - The expected blob type
    ///
    /// # Returns
    /// The blob data if found and type matches, None if not found or type mismatch, or error
    fn get_blob_of_type<'a>(
        &'a self,
        blob_id: &BlobId,
        expected_type: BlobType,
    ) -> BoxFuture<'a, error_stack::Result<Option<BlobData>, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            match self.get_blob(&blob_id).await {
                Ok(blob_data) => {
                    if blob_data.blob_type() == expected_type {
                        Ok(Some(blob_data))
                    } else {
                        Ok(None)
                    }
                }
                Err(e) => {
                    // Check if it's a "not found" error - if so, return None instead of error
                    // For now, we'll just propagate all errors
                    Err(e)
                }
            }
        }
        .boxed()
    }

    /// Retrieve the result of a step execution by step index.
    ///
    /// # Arguments
    /// * `run_id` - The unique identifier for the workflow execution
    /// * `step_idx` - The index of the step within the workflow (0-based)
    ///
    /// # Returns
    /// The execution result if found, or an error if not found
    fn get_step_result(
        &self,
        run_id: Uuid,
        step_idx: usize,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>>;

    /// List all step results for a workflow execution, ordered by step index.
    ///
    /// This is useful for workflow recovery and debugging to see which steps
    /// have completed and their results in workflow order.
    ///
    /// # Arguments
    /// * `run_id` - The unique identifier for the workflow execution
    ///
    /// # Returns
    /// A vector of (step_index, step_id, result) tuples, ordered by step_index
    fn list_step_results(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepResult>, StateError>>;

    // Workflow Management Methods

    /// Store a workflow as a blob and return its blob ID.
    ///
    /// # Arguments
    /// * `workflow` - The workflow to store
    ///
    /// # Returns
    /// The blob ID of the stored workflow
    fn store_flow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>>;

    /// Retrieve a workflow by its blob ID.
    ///
    /// # Arguments
    /// * `flow_id` - The blob ID of the workflow
    ///
    /// # Returns
    /// The workflow if found, or an error if not found
    fn get_flow(
        &self,
        flow_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>>;

    /// Get all flows with a specific name, ordered by creation time (newest first).
    ///
    /// # Arguments
    /// * `name` - The flow name to search for
    ///
    /// # Returns
    /// A vector of (flow_id, created_at) tuples for flows with the given name
    #[allow(clippy::type_complexity)]
    fn get_flows(
        &self,
        name: &str,
    ) -> BoxFuture<'_, error_stack::Result<Vec<(BlobId, chrono::DateTime<chrono::Utc>)>, StateError>>;

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
    fn get_named_flow(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<Option<WorkflowWithMetadata>, StateError>>;

    /// Create or update a workflow label.
    ///
    /// # Arguments
    /// * `name` - The workflow name (from workflow.name field)
    /// * `label` - The label name (like "production", "staging")
    /// * `flow_id` - The blob ID of the workflow
    ///
    /// # Returns
    /// Success if the label was created/updated
    fn create_or_update_label(
        &self,
        name: &str,
        label: &str,
        flow_id: BlobId,
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
    fn list_flow_names(&self) -> BoxFuture<'_, error_stack::Result<Vec<String>, StateError>>;

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

    /// Create a new run record.
    ///
    /// # Arguments
    /// * `run_id` - The unique identifier for the run
    /// * `flow_id` - Workflow blob ID
    /// * `workflow_name` - Optional workflow name (from workflow.name field)
    /// * `workflow_label` - Optional workflow label used for execution
    /// * `debug_mode` - Whether run is in debug mode
    /// * `input` - Input data as JSON
    ///
    /// # Returns
    /// Success if the run was created
    fn create_run(
        &self,
        run_id: Uuid,
        flow_id: BlobId,
        workflow_name: Option<&str>,
        workflow_label: Option<&str>,
        debug_mode: bool,
        input: ValueRef,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Update run status.
    ///
    /// # Arguments
    /// * `run_id` - The run identifier
    /// * `status` - The new status
    /// * `result` - Optional result data as JSON
    ///
    /// # Returns
    /// Success if the run was updated
    fn update_run_status(
        &self,
        run_id: Uuid,
        status: ExecutionStatus,
        result: Option<ValueRef>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Get run details.
    ///
    /// # Arguments
    /// * `run_id` - The run identifier
    ///
    /// # Returns
    /// The run details if found
    fn get_run(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<RunDetails>, StateError>>;

    /// List runs with optional filtering.
    ///
    /// # Arguments
    /// * `filters` - Optional filters for the query
    ///
    /// # Returns
    /// A vector of run summaries
    fn list_runs(
        &self,
        filters: &RunFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RunSummary>, StateError>>;

    /// Flush any pending write operations to persistent storage.
    ///
    /// This method ensures that all queued write operations are completed before returning.
    /// The `run_id` parameter is a hint to implementations about which writes to prioritize,
    /// but implementations may choose to flush all pending writes regardless of the hint.
    ///
    /// # Arguments
    /// * `run_id` - Hint about which run's writes to flush (may be ignored by implementation)
    ///
    /// # Returns
    /// Success when all relevant pending writes have been persisted
    fn flush_pending_writes(
        &self,
        run_id: Uuid,
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
        run_id: Uuid,
        steps: &[StepInfo],
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Update the status of a single step.
    ///
    /// This operation may be queued and batched by the implementation for performance.
    /// Use `flush_pending_writes()` if immediate persistence is required.
    /// For updating multiple steps efficiently, use `update_step_statuses()`.
    ///
    /// # Arguments
    /// * `run_id` - The unique identifier for the workflow execution
    /// * `step_index` - The index of the step to update
    /// * `status` - The new status for the step
    fn update_step_status(
        &self,
        run_id: Uuid,
        step_index: usize,
        status: stepflow_core::status::StepStatus,
    );

    /// Get all step info for an execution.
    fn get_step_info_for_execution(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>>;

    /// Get runnable steps for an execution based on current status.
    /// Note: This method returns steps based on persistent status only.
    /// Dependency checking should be done by the caller using workflow analysis.
    fn get_runnable_steps(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>>;

    // Batch Execution Management

    /// Create a new batch record.
    ///
    /// # Arguments
    /// * `batch_id` - Unique identifier for the batch
    /// * `flow_id` - Workflow blob ID
    /// * `flow_name` - Optional workflow name
    /// * `total_runs` - Total number of runs in the batch
    ///
    /// # Returns
    /// Success if the batch was created
    fn create_batch(
        &self,
        batch_id: Uuid,
        flow_id: BlobId,
        flow_name: Option<&str>,
        total_runs: usize,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Link a run to a batch with its input index.
    ///
    /// # Arguments
    /// * `batch_id` - Batch identifier
    /// * `run_id` - Run identifier
    /// * `batch_input_index` - Position in the batch input array
    ///
    /// # Returns
    /// Success if the run was linked to the batch
    fn add_run_to_batch(
        &self,
        batch_id: Uuid,
        run_id: Uuid,
        batch_input_index: usize,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Get batch metadata (without statistics).
    ///
    /// # Arguments
    /// * `batch_id` - Batch identifier
    ///
    /// # Returns
    /// Batch metadata if found
    fn get_batch(
        &self,
        batch_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<BatchMetadata>, StateError>>;

    /// Get batch statistics by querying associated runs.
    ///
    /// # Arguments
    /// * `batch_id` - Batch identifier
    ///
    /// # Returns
    /// Batch statistics computed from run states
    fn get_batch_statistics(
        &self,
        batch_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<BatchStatistics, StateError>>;

    /// Update batch status (for cancellation).
    ///
    /// # Arguments
    /// * `batch_id` - Batch identifier
    /// * `status` - New batch status
    ///
    /// # Returns
    /// Success if the batch status was updated
    fn update_batch_status(
        &self,
        batch_id: Uuid,
        status: BatchStatus,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// List batches with optional filtering.
    ///
    /// # Arguments
    /// * `filters` - Filters for batch queries
    ///
    /// # Returns
    /// A vector of batch metadata matching the filters
    fn list_batches(
        &self,
        filters: &BatchFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<BatchMetadata>, StateError>>;

    /// List runs belonging to a batch with their input indices.
    ///
    /// # Arguments
    /// * `batch_id` - Batch identifier
    /// * `filters` - Filters for run queries
    ///
    /// # Returns
    /// A vector of (run_summary, batch_input_index) tuples
    fn list_batch_runs(
        &self,
        batch_id: Uuid,
        filters: &RunFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<(RunSummary, usize)>, StateError>>;

    /// Get batch context for a specific run (if it belongs to a batch).
    ///
    /// # Arguments
    /// * `run_id` - Run identifier
    ///
    /// # Returns
    /// Optional (batch_id, batch_input_index) tuple if the run belongs to a batch
    fn get_run_batch_context(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<(Uuid, usize)>, StateError>>;
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
    /// The workflow blob ID
    pub flow_id: BlobId,
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
    pub flow_id: BlobId,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Summary information about a flow run.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub run_id: Uuid,
    pub flow_id: BlobId,
    pub flow_name: Option<String>,
    pub flow_label: Option<String>,
    pub status: ExecutionStatus,
    pub debug_mode: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Detailed flow run information including input and result.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunDetails {
    #[serde(flatten)]
    pub summary: RunSummary,
    pub input: ValueRef,
    pub result: Option<FlowResult>,
}

/// Filters for listing runs.
#[derive(Debug, Clone, Default)]
pub struct RunFilters {
    pub status: Option<ExecutionStatus>,
    pub flow_name: Option<String>,
    pub flow_label: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Batch execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum BatchStatus {
    /// Batch is currently running
    Running,
    /// Batch has been cancelled
    Cancelled,
}

impl Default for BatchStatus {
    fn default() -> Self {
        Self::Running
    }
}

/// Immutable batch metadata
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchMetadata {
    pub batch_id: Uuid,
    pub flow_id: BlobId,
    pub flow_name: Option<String>,
    pub total_runs: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub status: BatchStatus,
}

/// Calculated batch statistics (not stored, computed via queries)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchStatistics {
    pub completed_runs: usize,
    pub running_runs: usize,
    pub failed_runs: usize,
    pub cancelled_runs: usize,
    pub paused_runs: usize,
}

impl Default for BatchStatistics {
    fn default() -> Self {
        Self {
            completed_runs: 0,
            running_runs: 0,
            failed_runs: 0,
            cancelled_runs: 0,
            paused_runs: 0,
        }
    }
}

/// Complete batch details for API responses
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchDetails {
    #[serde(flatten)]
    pub metadata: BatchMetadata,
    #[serde(flatten)]
    pub statistics: BatchStatistics,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Filters for listing batches
#[derive(Debug, Clone, Default)]
pub struct BatchFilters {
    pub status: Option<BatchStatus>,
    pub flow_name: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Run details with resolved input and result blobs.
#[derive(Debug)]
pub struct RunWithBlobs {
    pub run: RunDetails,
    pub input: Option<ValueRef>,
    pub result: Option<ValueRef>,
}

/// Comprehensive run step details for server inspection.
#[derive(Debug)]
pub struct RunStepDetails {
    pub run: RunDetails,
    pub workflow: Option<Arc<stepflow_core::workflow::Flow>>,
    pub step_results: Vec<StepResult>,
    pub input: Option<ValueRef>,
}

/// Complete data needed for debug session creation.
#[derive(Debug)]
pub struct DebugSessionData {
    pub run: RunDetails,
    pub workflow: Arc<stepflow_core::workflow::Flow>,
    pub input: ValueRef,
    pub step_results: Vec<StepResult>,
}

/// Step information for a flow run.
#[derive(Debug, Clone, PartialEq)]
pub struct StepInfo {
    /// Run ID this step belongs to
    pub run_id: Uuid,
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
    use uuid::Uuid;

    #[test]
    fn test_run_details_serde_flatten() {
        let now = chrono::Utc::now();
        let run_id = Uuid::new_v4();
        let flow_id = BlobId::new("a".repeat(64)).unwrap();

        let details = RunDetails {
            summary: RunSummary {
                run_id,
                flow_id: flow_id.clone(),
                flow_name: Some("test-workflow".to_string()),
                flow_label: Some("production".to_string()),
                status: ExecutionStatus::Completed,
                debug_mode: false,
                created_at: now,
                completed_at: Some(now),
            },
            input: stepflow_core::workflow::ValueRef::new(json!({"test": "input"})),
            result: Some(FlowResult::Success(stepflow_core::workflow::ValueRef::new(
                json!({"test": "output"}),
            ))),
        };

        // Serialize the RunDetails
        let serialized = serde_json::to_string(&details).unwrap();

        // Parse as a generic JSON value to verify flattening
        let value: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        // Verify that summary fields are flattened to the top level
        assert_eq!(value["runId"], json!(run_id));
        assert_eq!(value["flowId"], json!(flow_id.as_str()));
        assert_eq!(value["flowName"], json!("test-workflow"));
        assert_eq!(value["flowLabel"], json!("production"));
        assert_eq!(value["status"], json!("completed"));
        assert_eq!(value["debugMode"], json!(false));

        // Verify that detail-specific fields are also present
        assert_eq!(value["input"], json!({"test": "input"}));
        assert_eq!(
            value["result"],
            json!({"outcome": "success", "result": {"test": "output"}})
        );

        // Verify there's no nested "summary" object
        assert!(value.get("summary").is_none());

        // Verify it deserializes back correctly
        let deserialized: RunDetails = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, details);
    }
}
