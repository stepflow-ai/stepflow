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

//! Data transfer types for Stepflow API and CLI.
//!
//! This crate provides DTOs (Data Transfer Objects) used across the Stepflow
//! system for representing run status, item results, and related data.
//! These types are pure data structures with no storage or execution logic.

use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::workflow::{Component, ValueRef};
use stepflow_core::{BlobId, FlowResult};
use uuid::Uuid;

// Re-export StepId for convenience
pub use stepflow_core::workflow::StepId;

/// Status information for a single step execution.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepStatusInfo {
    /// Step name/identifier.
    pub step_id: String,
    /// Current status of the step.
    pub status: StepStatus,
}

impl StepStatusInfo {
    /// Create a new step status info from a StepId.
    pub fn new(step_id: &StepId, status: StepStatus) -> Self {
        Self {
            step_id: step_id.name().to_string(),
            status,
        }
    }
}

/// Detailed information for a single item in a run.
///
/// Includes the item's input, execution status, and step-level details.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetails {
    /// Index of this item in the input array (0-based).
    pub item_index: u32,
    /// Input value for this item.
    pub input: ValueRef,
    /// Execution status of this item.
    pub status: ExecutionStatus,
    /// Step statuses for this item.
    pub steps: Vec<StepStatusInfo>,
    /// When this item completed (if completed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
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
}

/// Detailed flow run information including item details.
///
/// For completed runs, `item_details` contains per-item information including
/// inputs and step statuses. For active runs, `item_details` may be `None`
/// (query the owning orchestrator for live status).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunDetails {
    #[serde(flatten)]
    pub summary: RunSummary,
    /// Item details with inputs and step statuses.
    /// - `None`: details not requested, or run is active (query executor)
    /// - `Some`: item-level details available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_details: Option<Vec<ItemDetails>>,
    /// Optional workflow overrides applied to this run (per-run, not per-item).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<stepflow_core::workflow::WorkflowOverrides>,
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
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// Re-export ResultOrder from stepflow-core for convenience
pub use stepflow_core::ResultOrder;

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
    pub status: ExecutionStatus,
    /// Statistics about items in this run.
    pub items: ItemStatistics,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Root run ID for this execution tree.
    pub root_run_id: Uuid,
    /// Parent run ID if this is a sub-flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Uuid>,
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
            status: details.summary.status,
            items: details.summary.items.clone(),
            created_at: details.summary.created_at,
            completed_at: details.summary.completed_at,
            root_run_id: details.summary.root_run_id,
            parent_run_id: details.summary.parent_run_id,
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
            status: details.summary.status,
            items: details.summary.items.clone(),
            created_at: details.summary.created_at,
            completed_at: details.summary.completed_at,
            root_run_id: details.summary.root_run_id,
            parent_run_id: details.summary.parent_run_id,
            results: Some(items),
        }
    }

    /// Create RunStatus from RunSummary (for list operations).
    pub fn from_summary(summary: &RunSummary) -> Self {
        Self {
            run_id: summary.run_id,
            flow_id: summary.flow_id.clone(),
            flow_name: summary.flow_name.clone(),
            status: summary.status,
            items: summary.items.clone(),
            created_at: summary.created_at,
            completed_at: summary.completed_at,
            root_run_id: summary.root_run_id,
            parent_run_id: summary.parent_run_id,
            results: None,
        }
    }
}

/// The step result.
#[derive(PartialEq, Debug, Clone)]
pub struct StepResult {
    step_id: StepId,
    result: FlowResult,
}

impl StepResult {
    /// Create a new step result with a StepId.
    ///
    /// This is the primary constructor. The StepId can be created via:
    /// - `StepId::for_step(flow, index)` when you have the flow (preferred, no allocation)
    /// - `StepId::new(name, index)` when you only have the name and index
    pub fn new(step_id: StepId, result: FlowResult) -> Self {
        Self { step_id, result }
    }

    /// Get the step index.
    pub fn step_index(&self) -> usize {
        self.step_id.index()
    }

    /// Get the step name.
    pub fn step_name(&self) -> &str {
        self.step_id.name()
    }

    /// Get the StepId.
    pub fn step_id(&self) -> &StepId {
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
        self.step_id.partial_cmp(&other.step_id)
    }
}

/// Step information for a flow run.
#[derive(Debug, Clone, PartialEq)]
pub struct StepInfo {
    /// Run ID this step belongs to
    pub run_id: Uuid,
    /// Step identifier (index + name)
    pub step_id: StepId,
    /// Component name/URL
    pub component: Component,
    /// Current status of the step
    pub status: stepflow_core::status::StepStatus,
    /// When the step was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the step was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl StepInfo {
    /// Get the step index.
    pub fn step_index(&self) -> usize {
        self.step_id.index()
    }

    /// Get the step name.
    pub fn step_name(&self) -> &str {
        self.step_id.name()
    }
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
                status: ExecutionStatus::Completed,
                items: ItemStatistics::single(ExecutionStatus::Completed),
                created_at: now,
                completed_at: Some(now),
                root_run_id: run_id,
                parent_run_id: None,
                orchestrator_id: None,
            },
            item_details: Some(vec![ItemDetails {
                item_index: 0,
                input: stepflow_core::workflow::ValueRef::new(json!({"test": "input"})),
                status: ExecutionStatus::Completed,
                steps: vec![],
                completed_at: Some(now),
            }]),
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
        assert_eq!(value["status"], json!("completed"));

        // Verify that item_details field is present
        assert!(value.get("itemDetails").is_some());

        // Verify there's no nested "summary" object
        assert!(value.get("summary").is_none());

        // Verify it deserializes back correctly
        let deserialized: RunDetails = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, details);
    }
}
