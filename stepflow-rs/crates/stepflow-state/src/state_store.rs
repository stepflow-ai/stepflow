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

//! StateStore trait combining MetadataStore with step-level operations.
//!
//! This module provides backward compatibility by combining the new `MetadataStore`
//! trait with step-level operations that will eventually be replaced by journal-based
//! recovery.

use std::collections::HashMap;

use bit_set::BitSet;
use futures::future::{BoxFuture, FutureExt as _};
use stepflow_core::status::StepStatus;
use stepflow_core::{
    BlobData, BlobId, BlobType, FlowResult,
    workflow::{ValueRef, WorkflowOverrides},
};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::StateError;
use crate::metadata_store::MetadataStore;

use stepflow_dtos::{StepInfo, StepResult};

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
    /// Input data for the workflow execution (one per item)
    pub inputs: Vec<ValueRef>,
    /// Runtime overrides for step inputs
    pub overrides: WorkflowOverrides,
    /// Variables to provide for variable references in the workflow
    pub variables: HashMap<String, ValueRef>,
    /// Root run ID for this execution tree.
    ///
    /// For top-level runs, this equals `run_id`.
    /// For sub-flows, this is the original run that started the tree.
    pub root_run_id: Uuid,
    /// Parent run ID if this is a sub-flow.
    ///
    /// None for top-level runs, Some(parent_id) for sub-flows.
    pub parent_run_id: Option<Uuid>,
}

impl CreateRunParams {
    /// Create run params with the given inputs for a root (top-level) run.
    ///
    /// For single-item runs, pass `vec![input]`.
    /// Sets `root_run_id` equal to `run_id` and `parent_run_id` to None.
    pub fn new(run_id: Uuid, flow_id: BlobId, inputs: Vec<ValueRef>) -> Self {
        Self {
            run_id,
            flow_id,
            inputs,
            workflow_name: None,
            overrides: WorkflowOverrides::default(),
            variables: HashMap::new(),
            root_run_id: run_id,
            parent_run_id: None,
        }
    }

    /// Create run params for a sub-flow within an existing run tree.
    ///
    /// Inherits the `root_run_id` from the parent context and sets this run's
    /// parent to the provided `parent_run_id`.
    pub fn new_subflow(
        run_id: Uuid,
        flow_id: BlobId,
        inputs: Vec<ValueRef>,
        root_run_id: Uuid,
        parent_run_id: Uuid,
    ) -> Self {
        Self {
            run_id,
            flow_id,
            inputs,
            workflow_name: None,
            overrides: WorkflowOverrides::default(),
            variables: HashMap::new(),
            root_run_id,
            parent_run_id: Some(parent_run_id),
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

/// Write operations for async queuing.
///
/// These operations are primarily for step-level results and will eventually
/// be replaced by journal-based recording.
#[derive(Debug)]
pub enum StateWriteOperation {
    /// Create a new run record.
    ///
    /// This is used for subflow runs that need to be created synchronously
    /// before results can be recorded.
    CreateRun { params: CreateRunParams },
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
/// This trait extends [`MetadataStore`] with step-level operations for backward
/// compatibility. New code should prefer using `MetadataStore` directly where
/// step-level details are not needed, as step-level state will eventually be
/// recovered from the [`ExecutionJournal`](crate::ExecutionJournal) instead.
///
/// # Migration Path
///
/// The step-level methods (`get_step_result`, `list_step_results`, `initialize_run_steps`,
/// `update_step_status`, `get_step_info_for_run`) are being migrated to journal-based
/// recovery. During the transition:
///
/// 1. Existing code continues to use `StateStore` unchanged
/// 2. New recovery code uses `ExecutionJournal` for step-level details
/// 3. Eventually, step-level methods will be deprecated
pub trait StateStore: MetadataStore + Send + Sync {
    // =========================================================================
    // Blob Helper Methods
    // =========================================================================

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

    // =========================================================================
    // Step-Level Operations (to be migrated to ExecutionJournal)
    // =========================================================================

    /// Retrieve the result of a step execution by step index.
    ///
    /// **Note**: This method is being migrated to journal-based recovery.
    /// New code should use `ExecutionJournal` for step-level details.
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
    /// **Note**: This method is being migrated to journal-based recovery.
    /// New code should use `ExecutionJournal` for step-level details.
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

    // =========================================================================
    // Write Queue Operations
    // =========================================================================

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

    // =========================================================================
    // Step Status Management (to be migrated to ExecutionJournal)
    // =========================================================================

    /// Initialize step info for a run.
    ///
    /// **Note**: This method is being migrated to journal-based recovery.
    /// New code should use `ExecutionJournal` for step-level details.
    ///
    /// This should be called when starting execution of a run to create
    /// initial step entries with their starting status.
    fn initialize_run_steps(
        &self,
        run_id: Uuid,
        steps: &[StepInfo],
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Update the status of a single step.
    ///
    /// **Note**: This method is being migrated to journal-based recovery.
    /// New code should use `ExecutionJournal` for step-level details.
    ///
    /// This operation may be queued and batched by the implementation for performance.
    /// Use `flush_pending_writes()` if immediate persistence is required.
    ///
    /// # Arguments
    /// * `run_id` - The unique identifier for the run
    /// * `step_index` - The index of the step to update
    /// * `status` - The new status for the step
    fn update_step_status(
        &self,
        run_id: Uuid,
        step_index: usize,
        status: stepflow_core::status::StepStatus,
    );

    /// Get all step info for a run.
    ///
    /// **Note**: This method is being migrated to journal-based recovery.
    /// New code should use `ExecutionJournal` for step-level details.
    fn get_step_info_for_run(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>>;
}
