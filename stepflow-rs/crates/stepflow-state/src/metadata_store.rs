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

//! MetadataStore trait for persistent storage of run metadata and results.
//!
//! This trait handles durable storage of workflow state that needs to be accessible
//! across orchestrator instances. It stores:
//! - Run metadata (for RunDetails/RunSummary API responses)
//! - Item-level results
//!
//! Blob storage (content-addressed data and flow definitions) is handled by
//! the [`BlobStore`](crate::BlobStore) trait.
//!
//! Step-level details (StepResult, StepInfo, StepStatus) are recovered from the
//! ExecutionJournal during recovery, not stored in MetadataStore.

use futures::future::BoxFuture;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::{FlowResult, workflow::WorkflowOverrides};
use uuid::Uuid;

use crate::StateError;
use stepflow_dtos::{ItemResult, ResultOrder, RunDetails, RunFilters, RunSummary, StepStatusInfo};

use super::state_store::CreateRunParams;

/// Trait for storing and retrieving run metadata and results.
///
/// This trait provides the foundation for persistent storage of workflow
/// execution metadata. It is designed to be implemented by various backends
/// (SQLite, NATS KV, CockroachDB, etc.) with consistent semantics.
///
/// Blob storage is provided by the separate [`BlobStore`](crate::BlobStore) trait.
/// Step-level execution details are handled by [`ExecutionJournal`](crate::ExecutionJournal) and
/// recovered via journal replay during recovery.
pub trait MetadataStore: Send + Sync {
    // =========================================================================
    // Run Management
    // =========================================================================

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

    // =========================================================================
    // Item Results
    // =========================================================================

    /// Record the result of a single item in a multi-item run.
    ///
    /// This allows items to be persisted individually as they complete,
    /// rather than waiting to collect all results.
    ///
    /// # Arguments
    /// * `run_id` - The run identifier
    /// * `item_index` - The index of the item (0-based)
    /// * `result` - The result of the item execution
    /// * `step_statuses` - Status of each step that was executed for this item
    fn record_item_result(
        &self,
        run_id: Uuid,
        item_index: usize,
        result: FlowResult,
        step_statuses: Vec<StepStatusInfo>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

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

    // =========================================================================
    // Orchestrator Ownership
    // =========================================================================

    /// Update the orchestrator that owns a run.
    ///
    /// Set to `None` to mark the run as orphaned (available for claiming).
    /// Set to `Some(id)` to assign the run to a specific orchestrator.
    ///
    /// # Arguments
    /// * `run_id` - The run identifier
    /// * `orchestrator_id` - The new orchestrator owner, or None for orphaned
    fn update_run_orchestrator(
        &self,
        run_id: Uuid,
        orchestrator_id: Option<String>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Mark all Running runs owned by orchestrators NOT in the live set as orphaned.
    ///
    /// This is a batch operation equivalent to:
    /// ```sql
    /// UPDATE runs SET orchestrator_id = NULL
    /// WHERE orchestrator_id IS NOT NULL
    ///   AND orchestrator_id NOT IN (live_ids)
    ///   AND status = 'running'
    /// ```
    ///
    /// Returns the number of runs that were orphaned.
    ///
    /// # Accuracy of the live set
    ///
    /// The `live_orchestrator_ids` set does **not** need to be perfectly accurate.
    /// The lease manager is the source of truth for run ownership; the metadata
    /// store's `orchestrator_id` is an optimization for efficient discovery.
    ///
    /// - **Superset** (includes a dead orchestrator as live): The dead orchestrator's
    ///   runs won't be orphaned in this pass. They will be discovered in a future
    ///   iteration once the dead orchestrator is no longer in the live set. Safe,
    ///   but delays recovery.
    ///
    /// - **Subset** (misses a live orchestrator): The live orchestrator's runs get
    ///   incorrectly marked as orphaned. When another orchestrator tries to claim
    ///   them, `acquire_lease` returns `OwnedBy`, and the recovery module self-heals
    ///   by writing the actual owner back to the metadata store.
    ///
    /// # Arguments
    /// * `live_orchestrator_ids` - IDs of currently active orchestrators
    fn orphan_runs_by_stale_orchestrators(
        &self,
        live_orchestrator_ids: &[String],
    ) -> BoxFuture<'_, error_stack::Result<usize, StateError>>;

    // =========================================================================
    // Completion Notification
    // =========================================================================

    /// Wait for a run to reach a terminal status (Completed, Failed, or Cancelled).
    ///
    /// Returns immediately if the run is already in a terminal state.
    /// Returns an error if the run is not found.
    ///
    /// This method uses efficient notification rather than polling where possible.
    ///
    /// # Arguments
    /// * `run_id` - The run identifier to wait for
    fn wait_for_completion(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;
}
