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

use std::{collections::HashMap, sync::Arc};

use bit_set::BitSet;
use futures::future::{BoxFuture, FutureExt as _};
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{
    BlobData, BlobId, BlobType, FlowResult,
    workflow::{Component, Flow, StepId, ValueRef, WorkflowOverrides},
};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::StateError;

/// Parameters for creating a new workflow run.
///
/// A run can have one or more input items. Use `vec![input]` for single-item runs.
#[derive(Debug, Clone)]
pub struct CreateRunParams {
    /// Unique identifier for the workflow execution
    pub run_id: Uuid,
    /// The flow ID (blob hash) of the workflow to execute
    pub flow_id: BlobId,
    /// Optional workflow name for display and organization
    pub workflow_name: Option<String>,
    /// Optional workflow label for version identification
    pub workflow_label: Option<String>,
    /// Input data for the workflow execution (one per item)
    pub inputs: Vec<ValueRef>,
    /// Runtime overrides for step inputs
    pub overrides: WorkflowOverrides,
    /// Variables to provide for variable references in the workflow
    pub variables: HashMap<String, ValueRef>,
}

impl CreateRunParams {
    /// Create run params with the given inputs.
    ///
    /// For single-item runs, pass `vec![input]`.
    pub fn new(run_id: Uuid, flow_id: BlobId, inputs: Vec<ValueRef>) -> Self {
        Self {
            run_id,
            flow_id,
            inputs,
            workflow_name: None,
            workflow_label: None,
            overrides: WorkflowOverrides::default(),
            variables: HashMap::new(),
        }
    }

    /// Get the number of items in this run.
    pub fn item_count(&self) -> usize {
        self.inputs.len()
    }

    /// Get the input for a single-item run.
    /// Panics if this is a multi-item run.
    pub fn input(&self) -> &ValueRef {
        assert_eq!(self.inputs.len(), 1, "Expected single-item run");
        &self.inputs[0]
    }
}

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

    /// Create a new run record (idempotent).
    ///
    /// If a run with the same ID already exists, this is a no-op.
    /// This allows callers to ensure a run exists without tracking
    /// whether it was already created.
    ///
    /// # Arguments
    /// * `params` - Parameters for creating the run
    fn create_run(
        &self,
        params: CreateRunParams,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Get workflow overrides for a run.
    ///
    /// # Arguments
    /// * `run_id` - The run identifier
    ///
    /// # Returns
    /// The workflow overrides for the run (if any)
    fn get_run_overrides(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<WorkflowOverrides>, StateError>>;

    /// Update the status of a run.
    ///
    /// # Arguments
    /// * `run_id` - The run identifier
    /// * `status` - The new status
    ///
    /// # Returns
    /// Success if the run was updated
    fn update_run_status(
        &self,
        run_id: Uuid,
        status: ExecutionStatus,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Record the result of a single item in a multi-item run.
    ///
    /// This allows items to be persisted individually as they complete,
    /// rather than waiting to collect all results.
    ///
    /// # Arguments
    /// * `run_id` - The run identifier
    /// * `item_index` - The index of the item (0-based)
    /// * `result` - The result of the item execution
    fn record_item_result(
        &self,
        run_id: Uuid,
        item_index: usize,
        result: FlowResult,
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

    // Item Result Management

    /// Get all item results for a run.
    ///
    /// For single-item runs (item_count=1), returns a single item derived from run details.
    /// For multi-item runs, returns results from the item_results table.
    ///
    /// # Arguments
    /// * `run_id` - The run identifier
    /// * `order` - How to order the results (by index or by completion time)
    ///
    /// # Returns
    /// A vector of item results in the requested order
    fn get_item_results(
        &self,
        run_id: Uuid,
        order: ResultOrder,
    ) -> BoxFuture<'_, error_stack::Result<Vec<ItemResult>, StateError>>;
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

    /// Create a step result from a StepId.
    ///
    /// This extracts the step index and name from the StepId, avoiding
    /// the need to pass them separately.
    pub fn from_step_id(step_id: StepId, result: FlowResult) -> Self {
        Self {
            step_idx: step_id.index,
            step_id: step_id.step_name().to_string(),
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

/// Statistics about items in a run.
#[derive(
    Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ItemStatistics {
    /// Total number of items in this run.
    pub total: usize,
    /// Number of completed items.
    pub completed: usize,
    /// Number of currently running items.
    pub running: usize,
    /// Number of failed items.
    pub failed: usize,
    /// Number of cancelled items.
    pub cancelled: usize,
}

impl ItemStatistics {
    /// Create statistics for a single-item run with the given status.
    pub fn single(status: ExecutionStatus) -> Self {
        let mut stats = Self {
            total: 1,
            ..Default::default()
        };
        match status {
            ExecutionStatus::Completed => stats.completed = 1,
            ExecutionStatus::Running => stats.running = 1,
            ExecutionStatus::Failed => stats.failed = 1,
            ExecutionStatus::Cancelled => stats.cancelled = 1,
            ExecutionStatus::Paused => stats.running = 1, // Paused counts as running
        }
        stats
    }

    /// Check if all items are complete (none running).
    pub fn is_complete(&self) -> bool {
        self.running == 0
    }
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
    /// Statistics about items in this run.
    #[serde(default)]
    pub items: ItemStatistics,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Detailed flow run information including inputs.
///
/// Note: Item results are not included here. Use `get_item_results()` to
/// fetch item results separately. This design reflects that items are stored
/// in a separate table and avoids loading all results when just querying
/// run metadata.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunDetails {
    #[serde(flatten)]
    pub summary: RunSummary,
    /// Input values for each item in this run.
    pub inputs: Vec<ValueRef>,
    /// Optional workflow overrides applied to this run (per-run, not per-item).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<WorkflowOverrides>,
}

/// Filters for listing runs.
#[derive(Debug, Clone, Default)]
pub struct RunFilters {
    pub status: Option<ExecutionStatus>,
    pub flow_name: Option<String>,
    pub flow_label: Option<String>,
    /// Filter to runs under this root (includes the root itself).
    pub root_run_id: Option<Uuid>,
    /// Filter to direct children of this parent run.
    pub parent_run_id: Option<Uuid>,
    /// Maximum depth for hierarchy queries (0 = root only).
    pub max_depth: Option<u32>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Ordering for item results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultOrder {
    /// Return results in item index order (0, 1, 2, ...).
    #[default]
    ByIndex,
    /// Return results in completion order (first completed first).
    ByCompletion,
}

/// Result for an individual item in a multi-item run.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    /// Index of this item in the input array (0-based).
    pub item_index: usize,
    /// Execution status of this item.
    pub status: ExecutionStatus,
    /// Result of this item, if completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
    /// When this item completed (if completed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Unified run status returned by submit_run and get_run.
///
/// Contains status information and optionally item results.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunStatus {
    pub run_id: Uuid,
    pub flow_id: BlobId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_label: Option<String>,
    pub status: ExecutionStatus,
    /// Statistics about items in this run.
    pub items: ItemStatistics,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Item results, only populated if include_results=true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<ItemResult>>,
}

impl RunStatus {
    /// Create RunStatus from RunDetails without results.
    pub fn from_details(details: &RunDetails) -> Self {
        Self {
            run_id: details.summary.run_id,
            flow_id: details.summary.flow_id.clone(),
            flow_name: details.summary.flow_name.clone(),
            flow_label: details.summary.flow_label.clone(),
            status: details.summary.status,
            items: details.summary.items.clone(),
            created_at: details.summary.created_at,
            completed_at: details.summary.completed_at,
            results: None,
        }
    }

    /// Create RunStatus from RunDetails with item results.
    ///
    /// Item results should be fetched separately via `get_item_results()`.
    pub fn from_details_with_items(details: &RunDetails, items: Vec<ItemResult>) -> Self {
        Self {
            run_id: details.summary.run_id,
            flow_id: details.summary.flow_id.clone(),
            flow_name: details.summary.flow_name.clone(),
            flow_label: details.summary.flow_label.clone(),
            status: details.summary.status,
            items: details.summary.items.clone(),
            created_at: details.summary.created_at,
            completed_at: details.summary.completed_at,
            results: Some(items),
        }
    }

    /// Create RunStatus from RunSummary (for list operations).
    pub fn from_summary(summary: &RunSummary) -> Self {
        Self {
            run_id: summary.run_id,
            flow_id: summary.flow_id.clone(),
            flow_name: summary.flow_name.clone(),
            flow_label: summary.flow_label.clone(),
            status: summary.status,
            items: summary.items.clone(),
            created_at: summary.created_at,
            completed_at: summary.completed_at,
            results: None,
        }
    }
}

/// Run details with resolved input and result blobs.
#[derive(Debug)]
pub struct RunWithBlobs {
    pub run: RunDetails,
    pub inputs: Vec<Option<ValueRef>>,
    pub results: Vec<Option<ValueRef>>,
}

/// Comprehensive run step details for server inspection.
#[derive(Debug)]
pub struct RunStepDetails {
    pub run: RunDetails,
    pub workflow: Option<Arc<stepflow_core::workflow::Flow>>,
    pub step_results: Vec<StepResult>,
}

/// Complete data needed for debug session creation.
#[derive(Debug)]
pub struct DebugSessionData {
    pub run: RunDetails,
    pub workflow: Arc<stepflow_core::workflow::Flow>,
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
        let run_id = Uuid::now_v7();
        let flow_id = BlobId::new("a".repeat(64)).unwrap();

        let details = RunDetails {
            summary: RunSummary {
                run_id,
                flow_id: flow_id.clone(),
                flow_name: Some("test-workflow".to_string()),
                flow_label: Some("production".to_string()),
                status: ExecutionStatus::Completed,
                items: ItemStatistics::single(ExecutionStatus::Completed),
                created_at: now,
                completed_at: Some(now),
            },
            inputs: vec![stepflow_core::workflow::ValueRef::new(
                json!({"test": "input"}),
            )],
            overrides: None,
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

        // Verify that detail-specific fields are also present
        assert_eq!(value["inputs"], json!([{"test": "input"}]));

        // Verify there's no nested "summary" object
        assert!(value.get("summary").is_none());

        // Verify it deserializes back correctly
        let deserialized: RunDetails = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, details);
    }
}
