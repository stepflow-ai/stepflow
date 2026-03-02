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

//! Recovery module for resuming interrupted workflow executions.
//!
//! This module provides functionality to recover runs that were interrupted
//! due to crashes or restarts. Recovery works by:
//!
//! 1. Querying the **MetadataStore** for runs with `Running` status
//! 2. For each run, loading the flow definition from the BlobStore
//! 3. Replaying journal events to reconstruct the execution state
//! 4. Resuming execution from where it left off
//!
//! The MetadataStore is the source of truth for identifying recoverable runs.
//! The LeaseManager is not involved in discovery — it handles only coordination
//! (ownership enforcement) in multi-orchestrator deployments. This separation
//! allows the lease manager to remain a pure coordination primitive, compatible
//! with backends like etcd where expired keys are automatically deleted.
//!
//! ## Journal Organization
//!
//! Journals are keyed by `root_run_id`, meaning all events for an execution tree
//! (parent flow + all subflows) are stored in a single journal. During recovery,
//! we stream the journal from the run's creation sequence and apply events to
//! the affected `RunState`s using targeted dispatch via
//! [`JournalEvent::affected_run_ids`](stepflow_state::JournalEvent::affected_run_ids).
//!
//! ## Execution Tree Recovery
//!
//! A single `FlowExecutor` manages an entire execution tree (root run + subflows).
//! Recovery groups discovered runs by `root_run_id` and only recovers the root
//! run from each tree. Subflow runs don't need independent recovery because:
//! - Subflows whose parent step completed before the crash: the parent's
//!   `TaskCompleted` is in the journal, so the root already has their results.
//! - Subflows that were in-flight: the parent step was also in-flight and will
//!   be re-executed, re-submitting new subflows as needed.
//!
//! # Example
//!
//! ```ignore
//! use stepflow_execution::recover_orphaned_runs;
//!
//! // On startup, recover any orphaned runs
//! let recovered = recover_orphaned_runs(
//!     &env,            // environment with LeaseManager and ActiveExecutions
//!     orchestrator_id,
//!     100,             // max runs to recover
//! ).await?;
//!
//! println!("Recovered {} runs", recovered);
//! ```

mod restore;
mod tree;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_core::status::ExecutionStatus;
use stepflow_dtos::RunFilters;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    ActiveExecutionsExt as _, ExecutionJournalExt as _, LeaseManagerExt as _, LeaseResult,
    MetadataStoreExt as _, OrchestratorId, RunRecoveryInfo, SequenceNumber,
};

use crate::{ExecutionError, Result};

/// Result of a recovery operation.
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    /// Number of runs successfully recovered and resumed.
    pub recovered: usize,
    /// Number of runs that failed to recover.
    pub failed: usize,
    /// Run IDs that were recovered.
    pub recovered_run_ids: Vec<uuid::Uuid>,
    /// Run IDs that failed to recover with error messages.
    pub failed_runs: Vec<(uuid::Uuid, String)>,
}

impl RecoveryResult {
    /// Create a new empty recovery result.
    fn new() -> Self {
        Self {
            recovered: 0,
            failed: 0,
            recovered_run_ids: Vec::new(),
            failed_runs: Vec::new(),
        }
    }

    /// Record a successful recovery.
    fn record_success(&mut self, run_id: uuid::Uuid) {
        self.recovered += 1;
        self.recovered_run_ids.push(run_id);
    }

    /// Record a failed recovery.
    fn record_failure(&mut self, run_id: uuid::Uuid, error: String) {
        self.failed += 1;
        self.failed_runs.push((run_id, error));
    }
}

/// Recover pending runs on startup.
///
/// This function should be called during orchestrator startup to recover
/// any runs that were interrupted due to crashes or restarts.
///
/// The function:
/// 1. Claims runs for recovery via the lease manager
/// 2. For each run, loads the flow and replays journal events
/// 3. Resumes execution from where it left off
///
/// # Arguments
/// * `env` - The Stepflow environment (must have ActiveExecutions configured)
/// * `orchestrator_id` - This orchestrator's ID
/// * `limit` - Maximum number of runs to recover
///
/// # Returns
/// A `RecoveryResult` describing what was recovered
pub async fn recover_orphaned_runs(
    env: &Arc<StepflowEnvironment>,
    orchestrator_id: OrchestratorId,
    limit: usize,
) -> Result<RecoveryResult> {
    let lease_manager = env.lease_manager();
    let metadata_store = env.metadata_store().clone();
    let journal = env.execution_journal();

    // Find runs that need recovery by querying the metadata store for runs
    // with Running status, then attempting to acquire leases for each.
    // Only runs where the lease is successfully acquired are returned.
    let runs_to_recover =
        claim_for_recovery(lease_manager, &metadata_store, &orchestrator_id, limit).await?;

    // Filter out runs that are already being actively executed by this process.
    // This prevents the periodic recovery loop from re-recovering runs that are
    // in-progress (they show up as status=Running with our orchestrator_id).
    let active = env.active_executions();
    let runs_to_recover: Vec<_> = runs_to_recover
        .into_iter()
        .filter(|r| !active.contains(&r.root_run_id))
        .collect();

    if runs_to_recover.is_empty() {
        log::info!("No runs to recover");
        return Ok(RecoveryResult::new());
    }

    // Each RunRecoveryInfo is a root run (roots_only filter). Build a map
    // keyed by root_run_id for iteration.
    let trees: HashMap<uuid::Uuid, RunRecoveryInfo> = runs_to_recover
        .into_iter()
        .map(|info| (info.root_run_id, info))
        .collect();

    log::info!("Found {} execution trees to recover", trees.len());

    let mut result = RecoveryResult::new();

    for (root_run_id, root_info) in &trees {
        log::info!("Recovering execution tree rooted at {}", root_run_id);

        match tree::recover_execution_tree(env, journal, root_info).await {
            Ok(()) => {
                log::info!(
                    "Successfully recovered execution tree rooted at {}",
                    root_run_id
                );
                result.record_success(root_info.root_run_id);
            }
            Err(e) => {
                log::error!(
                    "Failed to recover execution tree rooted at {}: {:?}",
                    root_run_id,
                    e
                );
                result.record_failure(root_info.root_run_id, format!("{:?}", e));

                // Mark the root run as Failed so it won't be retried on subsequent
                // recovery attempts.
                if let Err(update_err) = metadata_store
                    .update_run_status(root_info.root_run_id, ExecutionStatus::Failed, None)
                    .await
                {
                    log::error!(
                        "Failed to mark run {} as failed after recovery error: {:?}",
                        root_info.root_run_id,
                        update_err
                    );
                }
            }
        }
    }

    log::info!(
        "Recovery complete: {} recovered, {} failed",
        result.recovered,
        result.failed
    );

    Ok(result)
}

/// Find runs that need recovery and acquire leases for them.
///
/// This queries the metadata store for runs with `Running` status that are
/// owned by this orchestrator (targeted query) or orphaned. For each run,
/// it attempts to acquire a lease via the lease manager. Only runs where the
/// lease is successfully acquired are returned — runs owned by other
/// orchestrators are skipped.
///
/// With deterministic orchestrator IDs (e.g., hostname-based), a restarting
/// orchestrator will find its own runs still leased under its ID. The same-owner
/// acquire path succeeds immediately, making restart recovery fast.
///
/// ## Self-healing
///
/// The lease manager is the source of truth for run ownership. The metadata
/// store's `orchestrator_id` is an optimization for efficient discovery, and
/// may become stale. This function self-heals in two directions:
///
/// - **Acquired an orphaned run**: writes our orchestrator ID to the metadata
///   store so future queries find it under our ownership.
/// - **OwnedBy another orchestrator**: writes the actual owner back to the
///   metadata store, correcting stale orphan status. This handles the case
///   where `orphan_runs_by_stale_orchestrators` was called with an incomplete
///   live set (missing the actual owner).
async fn claim_for_recovery(
    lease_manager: &Arc<dyn stepflow_state::LeaseManager>,
    metadata_store: &Arc<dyn stepflow_state::MetadataStore>,
    orchestrator_id: &OrchestratorId,
    limit: usize,
) -> Result<Vec<RunRecoveryInfo>> {
    // First, recover our own runs (targeted query by orchestrator_id).
    // This is the fast path for deterministic ID restarts.
    let own_filters = RunFilters {
        status: Some(ExecutionStatus::Running),
        orchestrator_id: Some(Some(orchestrator_id.as_str().to_string())),
        roots_only: Some(true),
        limit: Some(limit),
        ..Default::default()
    };
    let mut pending_runs = metadata_store
        .list_runs(&own_filters)
        .await
        .change_context(ExecutionError::RecoveryFailed)?;

    // Then, claim orphaned runs (orchestrator_id IS NULL) up to the remaining limit.
    let remaining = limit.saturating_sub(pending_runs.len());
    if remaining > 0 {
        let orphan_filters = RunFilters {
            status: Some(ExecutionStatus::Running),
            orchestrator_id: Some(None), // NULL = orphaned
            roots_only: Some(true),
            limit: Some(remaining),
            ..Default::default()
        };
        let orphaned_runs = metadata_store
            .list_runs(&orphan_filters)
            .await
            .change_context(ExecutionError::RecoveryFailed)?;
        pending_runs.extend(orphaned_runs);
    }

    let mut recovery_infos = Vec::with_capacity(pending_runs.len());

    for summary in pending_runs {
        debug_assert!(
            summary.parent_run_id.is_none(),
            "Should only recover root runs"
        );

        // Attempt to acquire the lease before loading details. This ensures only
        // one orchestrator recovers each run. With deterministic IDs, re-acquiring
        // our own lease (same owner) succeeds immediately.
        match lease_manager
            .acquire_lease(summary.run_id, orchestrator_id.clone())
            .await
        {
            Ok(LeaseResult::Acquired { .. }) => {
                // Lease acquired — update ownership if this was an orphaned run
                if summary.orchestrator_id.is_none()
                    && let Err(e) = metadata_store
                        .update_run_orchestrator(
                            summary.run_id,
                            Some(orchestrator_id.as_str().to_string()),
                        )
                        .await
                {
                    log::warn!(
                        "Failed to update orchestrator for run {}: {:?}",
                        summary.run_id,
                        e
                    );
                }
            }
            Ok(LeaseResult::OwnedBy { owner, .. }) => {
                // Self-heal: if the metadata store thinks this run is orphaned (or
                // owned by us) but the lease manager says otherwise, write the actual
                // owner back. This corrects stale orchestrator_id values caused by
                // orphan_runs_by_stale_orchestrators being called with an incomplete
                // live set.
                if summary.orchestrator_id.as_deref() != Some(owner.as_str())
                    && let Err(e) = metadata_store
                        .update_run_orchestrator(summary.run_id, Some(owner.as_str().to_string()))
                        .await
                {
                    log::warn!(
                        "Failed to self-heal orchestrator for run {}: {:?}",
                        summary.run_id,
                        e
                    );
                }
                log::debug!(
                    "Skipping run {} during recovery: owned by {}",
                    summary.run_id,
                    owner
                );
                continue;
            }
            Err(e) => {
                log::warn!(
                    "Failed to acquire lease for run {} during recovery: {:?}",
                    summary.run_id,
                    e
                );
                continue;
            }
        }

        recovery_infos.push(RunRecoveryInfo {
            root_run_id: summary.root_run_id,
            flow_id: summary.flow_id,
            start_sequence: SequenceNumber::new(summary.created_at_seqno),
        });
    }

    Ok(recovery_infos)
}
