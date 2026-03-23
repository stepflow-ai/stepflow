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

//! Domain types for Stepflow API, storage, and execution.
//!
//! This crate provides domain types used across the Stepflow system for
//! representing run status, item results, and related data.
//! These types are pure data structures with no storage or execution logic.

use stepflow_core::status::ExecutionStatus;
use stepflow_core::{BlobId, FlowResult};
use uuid::Uuid;

/// Detailed step status entry with result and journal tracking.
///
/// Stored in the `step_statuses` metadata table. Each entry tracks a single
/// step's status for a specific item, along with the journal sequence number
/// that produced the change (enabling `?asof=N` consistent reads).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepStatusEntry {
    /// Step name/identifier.
    pub step_id: String,
    /// Step index in the workflow definition.
    pub step_index: usize,
    /// Item index (0-based).
    pub item_index: u32,
    /// Current status of the step.
    pub status: stepflow_core::status::StepStatus,
    /// Component path (e.g., "/builtin/openai").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Step result (only present when completed or failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
    /// Journal sequence number of the event that produced this status.
    pub journal_seqno: u64,
    /// When this status was last updated.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Statistics about items in a run.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
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
            ExecutionStatus::RecoveryFailed => stats.failed = 1, // Recovery failure counts as failed
        }
        stats
    }

    /// Check if all items are complete (none running).
    pub fn is_complete(&self) -> bool {
        self.running == 0
    }
}

/// Summary information about a flow run.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub run_id: Uuid,
    pub flow_id: BlobId,
    pub flow_name: Option<String>,
    pub status: ExecutionStatus,
    /// Statistics about items in this run.
    #[serde(default)]
    pub items: ItemStatistics,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Root run ID for this execution tree.
    ///
    /// For top-level runs, this equals `run_id`.
    /// For sub-flows, this is the original run that started the tree.
    pub root_run_id: Uuid,
    /// Parent run ID if this is a sub-flow.
    ///
    /// None for top-level runs, Some(parent_id) for sub-flows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Uuid>,
    /// The orchestrator currently managing this run.
    ///
    /// None means the run is orphaned (no orchestrator owns it).
    /// Set when the run is created and updated during recovery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestrator_id: Option<String>,
    /// Journal sequence number of the event that created this run.
    ///
    /// Records where in the journal this run was created, enabling efficient
    /// metadata queries during recovery (filter by offset range).
    pub created_at_seqno: u64,
    /// Journal sequence number of the RunCompleted event for this run.
    ///
    /// Set when the run reaches a terminal state. `None` means the run is
    /// still in progress (or completed before this field was added).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at_seqno: Option<u64>,
}

/// Filters for listing runs.
#[derive(Debug, Clone, Default)]
pub struct RunFilters {
    pub status: Option<ExecutionStatus>,
    pub flow_name: Option<String>,
    /// Filter to runs under this root (includes the root itself).
    pub root_run_id: Option<Uuid>,
    /// Filter to direct children of this parent run.
    pub parent_run_id: Option<Uuid>,
    /// Filter to only root runs (runs where parent_run_id is None).
    pub roots_only: Option<bool>,
    /// Maximum depth for hierarchy queries (0 = root only).
    pub max_depth: Option<u32>,
    /// Filter by orchestrator ID.
    ///
    /// - `Some(Some(id))` — runs owned by a specific orchestrator
    /// - `Some(None)` — orphaned runs (orchestrator_id IS NULL)
    /// - `None` — no orchestrator filter applied
    pub orchestrator_id: Option<Option<String>>,
    /// Only return runs created at or after this journal sequence number.
    ///
    /// Useful for pruning recovery queries to sub-runs created after a
    /// checkpoint sequence number, avoiding a full scan of the tree.
    pub created_after_seqno: Option<u64>,
    /// Only return runs that haven't finished before this sequence number.
    ///
    /// Semantics: `finished_at_seqno IS NULL OR finished_at_seqno >= value`.
    /// This captures runs that are still in progress, or that finished at or
    /// after the given sequence number. Used during recovery to find sub-runs
    /// relevant to a checkpoint window.
    pub not_finished_before_seqno: Option<u64>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// Re-export ResultOrder from stepflow-core for convenience
pub use stepflow_core::ResultOrder;

/// Result for an individual item in a multi-item run.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

/// Run status combining summary metadata with optional item results.
///
/// Used by `get_run` and `submit_run` in the execution layer to return
/// run information along with optional results.
#[derive(Debug, Clone, PartialEq)]
pub struct RunStatus {
    pub run_id: Uuid,
    pub flow_id: BlobId,
    pub flow_name: Option<String>,
    pub status: ExecutionStatus,
    pub items: ItemStatistics,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub results: Option<Vec<ItemResult>>,
    pub root_run_id: Uuid,
    pub parent_run_id: Option<Uuid>,
    /// Journal sequence number of the event that created this run.
    pub created_at_seqno: u64,
}

impl RunStatus {
    /// Create a `RunStatus` from a `RunSummary` without item results.
    pub fn from_summary(s: &RunSummary) -> Self {
        Self {
            run_id: s.run_id,
            flow_id: s.flow_id.clone(),
            flow_name: s.flow_name.clone(),
            status: s.status,
            items: s.items.clone(),
            created_at: s.created_at,
            completed_at: s.completed_at,
            results: None,
            root_run_id: s.root_run_id,
            parent_run_id: s.parent_run_id,
            created_at_seqno: s.created_at_seqno,
        }
    }

    /// Create a `RunStatus` from a `RunSummary` with item results.
    pub fn from_summary_with_items(s: &RunSummary, items: Vec<ItemResult>) -> Self {
        Self {
            run_id: s.run_id,
            flow_id: s.flow_id.clone(),
            flow_name: s.flow_name.clone(),
            status: s.status,
            items: s.items.clone(),
            created_at: s.created_at,
            completed_at: s.completed_at,
            results: Some(items),
            root_run_id: s.root_run_id,
            parent_run_id: s.parent_run_id,
            created_at_seqno: s.created_at_seqno,
        }
    }
}

/// Simple step status pair used when recording item results.
///
/// This is a lightweight summary of step status, used to persist
/// per-item step statuses.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepStatusInfo {
    pub step_id: String,
    pub status: stepflow_core::status::StepStatus,
}
