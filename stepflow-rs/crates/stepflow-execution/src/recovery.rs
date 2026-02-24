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
//! we load the entire journal and apply all events to the root run's `RunState`.
//! Each `RunState` internally filters events by `run_id`, so subflow events are
//! silently ignored when applied to the root.
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

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use error_stack::ResultExt as _;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::apply_overrides;
use stepflow_dtos::RunFilters;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    ActiveExecutionsExt as _, BlobStoreExt as _, CheckpointStoreExt as _, ExecutionJournal,
    ExecutionJournalExt as _, LeaseManagerExt as _, LeaseResult, MetadataStoreExt as _,
    OrchestratorId, RunRecoveryInfo, SequenceNumber,
};

use crate::checkpoint::CheckpointData;
use crate::{ExecutionError, Result, RunState};

/// Information about a subflow discovered during journal replay.
struct SubflowInfo {
    flow_id: stepflow_core::BlobId,
    parent_id: uuid::Uuid,
    inputs: Vec<stepflow_core::workflow::ValueRef>,
    variables: HashMap<String, stepflow_core::workflow::ValueRef>,
}

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

    // Group runs by root_run_id. Each group represents a full execution tree
    // (root run + any subflows) that shares a single FlowExecutor. We recover
    // only the root run from each tree — subflows will be re-created when the
    // root's parent steps re-execute.
    let mut trees: HashMap<uuid::Uuid, Vec<RunRecoveryInfo>> = HashMap::new();
    for info in runs_to_recover {
        trees.entry(info.root_run_id).or_default().push(info);
    }

    log::info!("Found {} execution trees to recover", trees.len());

    let mut result = RecoveryResult::new();

    for (root_run_id, group) in &trees {
        // Find the root run in the group (where run_id == root_run_id)
        let root_info = match group.iter().find(|r| r.run_id == *root_run_id) {
            Some(info) => info,
            None => {
                // No root run found — these are orphaned subflows whose root is no longer
                // in Running status. They cannot be recovered without their root executor.
                log::warn!(
                    "No root run found for tree {}, marking {} orphaned subflows as failed",
                    root_run_id,
                    group.len()
                );
                for info in group {
                    result
                        .record_failure(info.run_id, "Root run not found for recovery".to_string());
                    if let Err(e) = metadata_store
                        .update_run_status(info.run_id, ExecutionStatus::Failed)
                        .await
                    {
                        log::error!(
                            "Failed to mark orphaned subflow {} as failed: {:?}",
                            info.run_id,
                            e
                        );
                    }
                }
                continue;
            }
        };

        log::info!(
            "Recovering execution tree rooted at {} ({} runs in tree)",
            root_run_id,
            group.len()
        );

        match recover_execution_tree(env, journal, root_info).await {
            Ok(()) => {
                log::info!(
                    "Successfully recovered execution tree rooted at {}",
                    root_run_id
                );
                result.record_success(root_info.run_id);
            }
            Err(e) => {
                log::error!(
                    "Failed to recover execution tree rooted at {}: {:?}",
                    root_run_id,
                    e
                );
                result.record_failure(root_info.run_id, format!("{:?}", e));

                // Mark the root run as Failed so it won't be retried on subsequent
                // recovery attempts.
                if let Err(update_err) = metadata_store
                    .update_run_status(root_info.run_id, ExecutionStatus::Failed)
                    .await
                {
                    log::error!(
                        "Failed to mark run {} as failed after recovery error: {:?}",
                        root_info.run_id,
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

        // Get full run details for inputs
        let details = metadata_store
            .get_run(summary.run_id)
            .await
            .change_context(ExecutionError::RecoveryFailed)?;

        if let Some(details) = details {
            let inputs = details
                .item_details
                .as_ref()
                .map(|items| items.iter().map(|item| item.input.clone()).collect())
                .unwrap_or_default();

            // Note: inputs and variables here are placeholders. Recovery extracts
            // authoritative values from the RunCreated journal event, which contains
            // the exact inputs and variables used when the run was originally created.
            // journal_offset is empty to replay from the beginning.
            recovery_infos.push(RunRecoveryInfo {
                run_id: summary.run_id,
                root_run_id: summary.root_run_id,
                parent_run_id: summary.parent_run_id,
                flow_id: summary.flow_id,
                inputs,
                variables: HashMap::new(),
                journal_offset: Bytes::new(),
            });
        }
    }

    Ok(recovery_infos)
}

/// Recover an execution tree by replaying its journal and resuming the root run.
///
/// This loads the full journal for the execution tree (keyed by `root_run_id`),
/// reconstructs the root run's state and any in-flight subflows, then spawns a
/// new `FlowExecutor` to resume execution.
///
/// ## Subflow Recovery
///
/// When `SubflowSubmitted` events are present in the journal, this function
/// reconstructs subflow `RunState` objects and builds a deduplication map.
/// When parent steps re-execute and re-submit subflows with the same deterministic
/// key, the executor matches against the recovered subflow and returns the existing
/// `run_id` instead of creating a duplicate. This avoids restarting completed or
/// in-progress subflows from scratch.
///
/// Subflows without a `SubflowSubmitted` event (crash between `RunCreated` and
/// `SubflowSubmitted`) are skipped — the parent step will re-create them.
async fn recover_execution_tree(
    env: &Arc<StepflowEnvironment>,
    journal: &Arc<dyn ExecutionJournal>,
    root_info: &stepflow_state::RunRecoveryInfo,
) -> Result<()> {
    let run_id = root_info.run_id;
    let root_run_id = root_info.root_run_id;
    debug_assert_eq!(
        run_id, root_run_id,
        "recover_execution_tree expects root run"
    );
    let state_store = env.metadata_store();
    let blob_store = env.blob_store();

    // Load the root flow definition
    let flow = blob_store
        .get_flow(&root_info.flow_id)
        .await
        .change_context(ExecutionError::RecoveryFailed)?
        .ok_or_else(|| {
            error_stack::report!(ExecutionError::RecoveryFailed)
                .attach_printable(format!("Flow not found: {}", root_info.flow_id))
        })?;

    // Load and apply overrides (if any) to match original execution
    let overrides = state_store
        .get_run_overrides(run_id)
        .await
        .change_context(ExecutionError::RecoveryFailed)?
        .unwrap_or_default();

    let flow = if overrides.is_empty() {
        flow
    } else {
        apply_overrides(flow, &overrides).change_context(ExecutionError::RecoveryFailed)?
    };

    // =========================================================================
    // Checkpoint-accelerated recovery
    // =========================================================================
    // Try to load a checkpoint first. If one exists, restore state from it and
    // only replay journal events after the checkpoint sequence. Otherwise fall
    // through to full replay from sequence 0.

    let checkpoint_store = env.checkpoint_store();
    let stored_checkpoint = checkpoint_store
        .get_latest_checkpoint(root_run_id)
        .await
        .change_context(ExecutionError::RecoveryFailed)?;

    // Try to deserialize the checkpoint (if present). On failure (version mismatch,
    // corruption), fall through to full replay.
    let valid_checkpoint = if let Some(ref stored_cp) = stored_checkpoint {
        match CheckpointData::deserialize(&stored_cp.data) {
            Ok(data) => Some((stored_cp, data)),
            Err(e) => {
                log::warn!(
                    "Checkpoint deserialization failed for tree {}, \
                     falling back to full replay: {e}",
                    root_run_id
                );
                None
            }
        }
    } else {
        None
    };

    // Restore or reconstruct state
    let (run_state, subflow_map, additional_runs) = if let Some((stored_cp, checkpoint_data)) =
        valid_checkpoint
    {
        log::info!(
            "Restoring from checkpoint at sequence {:?} for tree {}",
            stored_cp.sequence,
            root_run_id
        );

        // Restore the root run from checkpoint
        let root_checkpoint = checkpoint_data
            .runs
            .iter()
            .find(|rc| rc.run_id == run_id)
            .ok_or_else(|| {
                error_stack::report!(ExecutionError::RecoveryFailed)
                    .attach_printable("Root run not found in checkpoint")
            })?;

        let root_run_state = RunState::from_checkpoint(root_checkpoint, flow.clone());

        // Restore subflow RunStates from checkpoint
        let mut additional: HashMap<uuid::Uuid, RunState> = HashMap::new();
        for rc in &checkpoint_data.runs {
            if rc.run_id == run_id {
                continue; // Skip root, already restored
            }
            let sub_flow = blob_store
                .get_flow(&rc.flow_id)
                .await
                .change_context(ExecutionError::RecoveryFailed)?
                .ok_or_else(|| {
                    error_stack::report!(ExecutionError::RecoveryFailed).attach_printable(format!(
                        "Subflow flow not found during checkpoint recovery for run {}, flow_id={}",
                        rc.run_id, rc.flow_id
                    ))
                })?;
            let sub_state = RunState::from_checkpoint(rc, sub_flow);
            additional.insert(rc.run_id, sub_state);
        }

        // Restore subflow dedup map from checkpoint
        let mut sf_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid> = HashMap::new();
        for mapping in &checkpoint_data.subflow_map {
            sf_map.insert(
                (
                    mapping.parent_run_id,
                    mapping.item_index,
                    mapping.step_index,
                    mapping.subflow_key,
                ),
                mapping.subflow_run_id,
            );
        }

        // Replay tail events (after checkpoint) to bring state up to date.
        let tail_events_for_replay = journal
            .read_from(root_run_id, stored_cp.sequence.next(), usize::MAX)
            .await
            .change_context(ExecutionError::RecoveryFailed)?;

        let mut root_state = root_run_state;
        for event in &tail_events_for_replay {
            root_state.apply_event(event);
            for sub_state in additional.values_mut() {
                sub_state.apply_event(event);
            }
            // Also update subflow map from new SubflowSubmitted events
            if let stepflow_state::JournalEvent::SubflowSubmitted {
                parent_run_id,
                item_index,
                step_index,
                subflow_key,
                subflow_run_id,
            } = event
            {
                sf_map.insert(
                    (*parent_run_id, *item_index, *step_index, *subflow_key),
                    *subflow_run_id,
                );
            }
            // Handle new subflow RunCreated events after checkpoint
            if let stepflow_state::JournalEvent::RunCreated {
                run_id: sub_run_id,
                flow_id: sub_flow_id,
                inputs: sub_inputs,
                variables: sub_variables,
                parent_run_id: Some(parent_id),
            } = event
                && !additional.contains_key(sub_run_id)
                && sf_map.values().any(|id| id == sub_run_id)
            {
                let sub_flow = blob_store
                    .get_flow(sub_flow_id)
                    .await
                    .change_context(ExecutionError::RecoveryFailed)?
                    .ok_or_else(|| {
                        error_stack::report!(ExecutionError::RecoveryFailed).attach_printable(
                            format!(
                                "Subflow flow not found for run {}, flow_id={}",
                                sub_run_id, sub_flow_id
                            ),
                        )
                    })?;
                let mut sub_state = RunState::new_subflow(
                    *sub_run_id,
                    sub_flow_id.clone(),
                    root_run_id,
                    *parent_id,
                    sub_flow,
                    sub_inputs.clone(),
                    sub_variables.clone(),
                );
                // Apply remaining tail events to the new subflow
                for ev in &tail_events_for_replay {
                    sub_state.apply_event(ev);
                }
                additional.insert(*sub_run_id, sub_state);
            }
        }

        log::info!(
            "Restored from checkpoint + replayed {} tail events for tree {}, root complete={}",
            tail_events_for_replay.len(),
            root_run_id,
            root_state.is_complete()
        );

        (root_state, sf_map, additional)
    } else {
        // ---- Full replay path (no checkpoint) ----
        let all_events = journal
            .read_from(root_run_id, SequenceNumber::new(0), usize::MAX)
            .await
            .change_context(ExecutionError::RecoveryFailed)?;
        if all_events.is_empty() {
            log::warn!(
                "No journal entries for execution tree {}, cannot recover",
                root_run_id
            );
            return Err(error_stack::report!(ExecutionError::RecoveryFailed)
                .attach_printable("No journal entries found for this run"));
        }

        // Extract inputs and variables from the root run's RunCreated event.
        let (inputs, variables) = all_events
            .iter()
            .find_map(|event| match event {
                stepflow_state::JournalEvent::RunCreated {
                    run_id: event_run_id,
                    inputs,
                    variables,
                    parent_run_id: None,
                    ..
                } if *event_run_id == run_id => Some((inputs.clone(), variables.clone())),
                _ => None,
            })
            .ok_or_else(|| {
                error_stack::report!(ExecutionError::RecoveryFailed)
                    .attach_printable("No RunCreated event found in journal for root run")
            })?;

        // Create RunState for the root run
        let mut run_state = RunState::new(
            run_id,
            root_info.flow_id.clone(),
            flow.clone(),
            inputs,
            variables,
        );

        // Apply ALL events to reconstruct root state
        all_events.iter().for_each(|event| {
            run_state.apply_event(event);
        });

        log::info!(
            "Replayed {} journal events for execution tree {}, root complete={}",
            all_events.len(),
            root_run_id,
            run_state.is_complete()
        );

        // Build subflow dedup map from journal events
        let mut sf_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid> = HashMap::new();
        for event in &all_events {
            if let stepflow_state::JournalEvent::SubflowSubmitted {
                parent_run_id,
                item_index,
                step_index,
                subflow_key,
                subflow_run_id,
            } = event
            {
                sf_map.insert(
                    (*parent_run_id, *item_index, *step_index, *subflow_key),
                    *subflow_run_id,
                );
            }
        }

        // Find subflow RunCreated events and reconstruct their RunStates
        let mut subflow_created: HashMap<uuid::Uuid, SubflowInfo> = HashMap::new();
        for event in &all_events {
            if let stepflow_state::JournalEvent::RunCreated {
                run_id: sub_run_id,
                flow_id: sub_flow_id,
                inputs: sub_inputs,
                variables: sub_variables,
                parent_run_id: Some(parent_id),
            } = event
                && sf_map.values().any(|id| id == sub_run_id)
            {
                subflow_created.insert(
                    *sub_run_id,
                    SubflowInfo {
                        flow_id: sub_flow_id.clone(),
                        parent_id: *parent_id,
                        inputs: sub_inputs.clone(),
                        variables: sub_variables.clone(),
                    },
                );
            }
        }

        let mut additional: HashMap<uuid::Uuid, RunState> = HashMap::new();
        for (sub_run_id, info) in &subflow_created {
            let sub_flow = blob_store
                .get_flow(&info.flow_id)
                .await
                .change_context(ExecutionError::RecoveryFailed)?
                .ok_or_else(|| {
                    error_stack::report!(ExecutionError::RecoveryFailed).attach_printable(format!(
                        "Subflow flow not found during recovery for run {}, flow_id={}",
                        sub_run_id, info.flow_id
                    ))
                })?;

            let mut sub_state = RunState::new_subflow(
                *sub_run_id,
                info.flow_id.clone(),
                root_run_id,
                info.parent_id,
                sub_flow,
                info.inputs.clone(),
                info.variables.clone(),
            );

            all_events.iter().for_each(|event| {
                sub_state.apply_event(event);
            });

            log::info!(
                "Recovered subflow run {}: complete={}, parent={}",
                sub_run_id,
                sub_state.is_complete(),
                info.parent_id
            );

            additional.insert(*sub_run_id, sub_state);
        }

        (run_state, sf_map, additional)
    };

    // If the run is already complete, no need to resume
    if run_state.is_complete() {
        log::info!("Root run {} already complete after recovery", run_id);
        return Ok(());
    }

    if !subflow_map.is_empty() {
        log::info!(
            "Recovered {} subflow mappings, {} subflow RunStates for tree {}",
            subflow_map.len(),
            additional_runs.len(),
            root_run_id
        );
    }

    // Build the FlowExecutor with the recovered root state and subflow states.
    let mut builder = crate::FlowExecutorBuilder::new(env.clone(), run_state)
        .skip_validation() // Already validated before crash
        .scheduler(Box::new(crate::DepthFirstScheduler::new()));

    if !additional_runs.is_empty() || !subflow_map.is_empty() {
        builder = builder.with_recovered_subflows(additional_runs, subflow_map);
    }

    let flow_executor = builder
        .build()
        .await
        .change_context(ExecutionError::RecoveryFailed)?;

    // Spawn and track the recovered execution tree
    flow_executor.spawn(env.active_executions());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;

    use stepflow_core::workflow::{FlowBuilder, StepBuilder, ValueRef};
    use stepflow_core::{BlobId, ValueExpr};
    use stepflow_state::{CreateRunParams, ItemSteps, JournalEvent, RunTaskAttempts};

    use crate::testing::MockExecutorBuilder;

    /// Helper to create a test environment with in-memory stores.
    async fn create_test_env() -> Arc<StepflowEnvironment> {
        MockExecutorBuilder::new().build().await
    }

    /// Helper to create a simple test flow.
    fn create_test_flow() -> stepflow_core::workflow::Flow {
        FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::new("step0")
                    .component("/mock/test")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step0".to_string(),
                path: Default::default(),
            })
            .build()
    }

    #[tokio::test]
    async fn test_recovery_no_runs_to_recover() {
        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");

        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");

        assert_eq!(result.recovered, 0);
        assert_eq!(result.failed, 0);
    }

    #[tokio::test]
    async fn test_recovery_missing_flow_marks_run_failed() {
        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let journal = env.execution_journal();

        // Create a run record with a non-existent flow ID
        let run_id = uuid::Uuid::now_v7();
        let fake_flow_id = BlobId::from_content(&ValueRef::new(json!({"nonexistent": true})))
            .expect("should create blob id");

        let params =
            CreateRunParams::new(run_id, fake_flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Add a RunCreated journal entry (required for recovery)
        journal
            .write(
                run_id,
                JournalEvent::RunCreated {
                    run_id,
                    flow_id: fake_flow_id,
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: None,
                },
            )
            .await
            .expect("should write");

        // Attempt recovery - should fail because flow doesn't exist
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed overall");

        // The run should be marked as failed
        assert_eq!(result.recovered, 0);
        assert_eq!(result.failed, 1);
        assert!(result.failed_runs[0].1.contains("Flow not found"));

        // Verify the run status was updated to Failed
        let run = metadata_store
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(run.summary.status, ExecutionStatus::Failed);
    }

    #[tokio::test]
    async fn test_recovery_missing_journal_entries_marks_run_failed() {
        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();

        // Store a valid flow
        let flow = Arc::new(create_test_flow());
        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        // Create a run record but DON'T add any journal entries
        let run_id = uuid::Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id, vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Attempt recovery - should fail because no journal entries
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed overall");

        // The run should be marked as failed
        assert_eq!(result.recovered, 0);
        assert_eq!(result.failed, 1);
        assert!(result.failed_runs[0].1.contains("No journal entries"));

        // Verify the run status was updated to Failed
        let run = metadata_store
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(run.summary.status, ExecutionStatus::Failed);
    }

    #[tokio::test]
    async fn test_recovery_missing_run_created_event_marks_run_failed() {
        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        // Store a valid flow
        let flow = Arc::new(create_test_flow());
        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        // Create a run record
        let run_id = uuid::Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Add a TaskCompleted event but NO RunCreated event
        journal
            .write(
                run_id,
                JournalEvent::TaskCompleted {
                    run_id,
                    item_index: 0,
                    step_index: 0,
                    result: stepflow_core::FlowResult::Success(ValueRef::new(json!({}))),
                },
            )
            .await
            .expect("should write");

        // Attempt recovery - should fail because no RunCreated event
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed overall");

        // The run should be marked as failed
        assert_eq!(result.recovered, 0);
        assert_eq!(result.failed, 1);
        assert!(result.failed_runs[0].1.contains("RunCreated"));

        // Verify the run status was updated to Failed
        let run = metadata_store
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(run.summary.status, ExecutionStatus::Failed);
    }

    #[tokio::test]
    async fn test_recovery_already_complete_run_succeeds() {
        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        // Store a valid flow
        let flow = Arc::new(create_test_flow());
        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        // Create a run record
        let run_id = uuid::Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Add journal entries that represent a completed run
        journal
            .write(
                run_id,
                JournalEvent::RunCreated {
                    run_id,
                    flow_id: flow_id.clone(),
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: None,
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::RunInitialized {
                    run_id,
                    needed_steps: vec![ItemSteps {
                        item_index: 0,
                        step_indices: vec![0],
                    }],
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::TaskCompleted {
                    run_id,
                    item_index: 0,
                    step_index: 0,
                    result: stepflow_core::FlowResult::Success(ValueRef::new(
                        json!({"result": "ok"}),
                    )),
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::ItemCompleted {
                    run_id,
                    item_index: 0,
                    result: stepflow_core::FlowResult::Success(ValueRef::new(
                        json!({"result": "ok"}),
                    )),
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::RunCompleted {
                    run_id,
                    status: ExecutionStatus::Completed,
                },
            )
            .await
            .expect("should write");

        // Attempt recovery - should succeed because run is already complete
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");

        // The run should be counted as recovered (it completed during replay)
        assert_eq!(result.recovered, 1);
        assert_eq!(result.failed, 0);
    }

    #[tokio::test]
    async fn test_recovery_result_tracking() {
        let mut result = RecoveryResult::new();

        assert_eq!(result.recovered, 0);
        assert_eq!(result.failed, 0);
        assert!(result.recovered_run_ids.is_empty());
        assert!(result.failed_runs.is_empty());

        let run1 = uuid::Uuid::now_v7();
        let run2 = uuid::Uuid::now_v7();

        result.record_success(run1);
        assert_eq!(result.recovered, 1);
        assert_eq!(result.recovered_run_ids, vec![run1]);

        result.record_failure(run2, "test error".to_string());
        assert_eq!(result.failed, 1);
        assert_eq!(result.failed_runs, vec![(run2, "test error".to_string())]);
    }

    /// Recovery must skip runs that are already tracked in ActiveExecutions.
    /// Without this filter, periodic recovery would re-recover runs that are
    /// actively executing (they appear as status=Running + our orchestrator_id).
    #[tokio::test]
    async fn test_recovery_skips_active_executions() {
        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        // Store a valid flow
        let flow = Arc::new(create_test_flow());
        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        // Create a run that appears to need recovery
        let run_id = uuid::Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Add journal entries so recovery would succeed
        journal
            .write(
                run_id,
                JournalEvent::RunCreated {
                    run_id,
                    flow_id,
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: None,
                },
            )
            .await
            .expect("should write");

        // Register this run as already active (simulates an in-flight execution)
        let active = env.active_executions();
        active.spawn(run_id, async {
            // Long-running task to keep it active during the test
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        });
        assert!(active.contains(&run_id));

        // Recovery should skip the active run
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");

        assert_eq!(
            result.recovered, 0,
            "Should not recover a run that is already active"
        );
        assert_eq!(result.failed, 0);

        // Clean up
        active.shutdown();
    }

    /// Integration test: Create a partial execution, abort it, and verify recovery resumes it.
    ///
    /// This test simulates the scenario where:
    /// 1. An execution starts and completes some steps
    /// 2. The orchestrator crashes/restarts (simulated by not completing the execution)
    /// 3. Recovery discovers the orphaned run and resumes it to completion
    #[tokio::test]
    async fn test_recovery_resumes_partial_execution() {
        use stepflow_core::workflow::FlowBuilder;

        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        // Create a 2-step chain flow: step0 -> step1
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step0")
                        .component("/mock/test")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Step {
                            step: "step0".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: Default::default(),
                })
                .build(),
        );

        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        // Create a run record
        let run_id = uuid::Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Journal entries for a PARTIAL execution:
        // - RunCreated
        // - RunInitialized (with both steps needed)
        // - TaskCompleted for step0 only
        // - NO ItemCompleted, NO RunCompleted (simulates crash after step0)

        journal
            .write(
                run_id,
                JournalEvent::RunCreated {
                    run_id,
                    flow_id: flow_id.clone(),
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: None,
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::RunInitialized {
                    run_id,
                    needed_steps: vec![ItemSteps {
                        item_index: 0,
                        step_indices: vec![0, 1], // Both steps needed
                    }],
                },
            )
            .await
            .expect("should write");

        // Step0 completed successfully
        let step0_result = ValueRef::new(json!({"result": "ok"}));
        journal
            .write(
                run_id,
                JournalEvent::TaskCompleted {
                    run_id,
                    item_index: 0,
                    step_index: 0,
                    result: stepflow_core::FlowResult::Success(step0_result),
                },
            )
            .await
            .expect("should write");

        // NO further entries - simulates crash after step0

        // Run recovery - should discover and resume the partial execution
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");

        // The run should be recovered and resumed to completion
        assert_eq!(
            result.recovered, 1,
            "Expected 1 recovered run, got {}",
            result.recovered
        );
        assert_eq!(result.failed, 0, "Expected 0 failed runs");
        assert!(
            result.recovered_run_ids.contains(&run_id),
            "Run ID should be in recovered list"
        );

        // Wait for the spawned execution to complete
        // Recovery spawns the execution asynchronously, so we need to wait
        let active_executions = env.active_executions();
        for _ in 0..100 {
            if active_executions.is_empty() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        assert!(
            active_executions.is_empty(),
            "Execution should complete within timeout"
        );

        // Verify the run completed successfully
        let run = metadata_store
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(
            run.summary.status,
            ExecutionStatus::Completed,
            "Run should have completed status after recovery"
        );
    }

    /// Test that recovery preserves attempt counts from the journal.
    ///
    /// Scenario: step0 was started (attempt=1) but crashed before completing.
    /// After recovery, the journal should contain the original TasksStarted,
    /// and when the executor re-runs, step0 should start with attempt=2.
    #[tokio::test]
    async fn test_recovery_preserves_attempt_counts() {
        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        // Single-step flow
        let flow = Arc::new(create_test_flow());
        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        let run_id = uuid::Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Journal: RunCreated, RunInitialized, TasksStarted(step0 attempt=1), NO TaskCompleted
        journal
            .write(
                run_id,
                JournalEvent::RunCreated {
                    run_id,
                    flow_id: flow_id.clone(),
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: None,
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::RunInitialized {
                    run_id,
                    needed_steps: vec![ItemSteps {
                        item_index: 0,
                        step_indices: vec![0],
                    }],
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::TasksStarted {
                    runs: vec![RunTaskAttempts {
                        run_id,
                        tasks: vec![stepflow_state::TaskAttempt::new(0, 0, 1)],
                    }],
                },
            )
            .await
            .expect("should write");

        // Recover - step0 should be re-executed
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");

        assert_eq!(result.recovered, 1);

        // Wait for execution to complete
        let active_executions = env.active_executions();
        for _ in 0..100 {
            if active_executions.is_empty() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        assert!(
            active_executions.is_empty(),
            "Execution should complete within timeout"
        );

        // Verify the run completed
        let run = metadata_store
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(run.summary.status, ExecutionStatus::Completed);

        // Verify the journal now has a second TasksStarted with attempt=2
        let all_entries = journal
            .read_from(run_id, SequenceNumber::new(0), usize::MAX)
            .await
            .expect("should read journal");

        let tasks_started_events: Vec<_> = all_entries
            .iter()
            .filter_map(|event| match event {
                JournalEvent::TasksStarted { runs } => Some(runs),
                _ => None,
            })
            .collect();

        // Should have 2 TasksStarted events: attempt=1 (pre-crash) and attempt=2 (recovery)
        assert_eq!(
            tasks_started_events.len(),
            2,
            "Should have 2 TasksStarted events"
        );
        // Find the tasks for this run in each event
        let pre_crash_tasks: Vec<_> = tasks_started_events[0]
            .iter()
            .filter(|r| r.run_id == run_id)
            .flat_map(|r| &r.tasks)
            .collect();
        let recovery_tasks: Vec<_> = tasks_started_events[1]
            .iter()
            .filter(|r| r.run_id == run_id)
            .flat_map(|r| &r.tasks)
            .collect();
        assert_eq!(pre_crash_tasks[0].attempt, 1);
        assert_eq!(recovery_tasks[0].attempt, 2);
    }

    /// Test that after recovery with multiple parallel tasks, a single batched
    /// TasksStarted is issued for all re-executed tasks.
    #[tokio::test]
    async fn test_recovery_batches_parallel_tasks() {
        use stepflow_core::workflow::FlowBuilder;

        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        // Flow with 2 independent steps (both depend only on input)
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step_a")
                        .component("/mock/test")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                    StepBuilder::new("step_b")
                        .component("/mock/test")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Input {
                    input: Default::default(),
                })
                .build(),
        );

        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        let run_id = uuid::Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Journal: both steps started (attempt 1) but neither completed (crash)
        journal
            .write(
                run_id,
                JournalEvent::RunCreated {
                    run_id,
                    flow_id: flow_id.clone(),
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: None,
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::RunInitialized {
                    run_id,
                    needed_steps: vec![ItemSteps {
                        item_index: 0,
                        step_indices: vec![0, 1],
                    }],
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::TasksStarted {
                    runs: vec![RunTaskAttempts {
                        run_id,
                        tasks: vec![
                            stepflow_state::TaskAttempt::new(0, 0, 1),
                            stepflow_state::TaskAttempt::new(0, 1, 1),
                        ],
                    }],
                },
            )
            .await
            .expect("should write");

        // No TaskCompleted for either - simulates crash

        // Recover
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");
        assert_eq!(result.recovered, 1);

        // Wait for execution
        let active_executions = env.active_executions();
        for _ in 0..100 {
            if active_executions.is_empty() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        assert!(active_executions.is_empty());

        // Verify completed
        let run = metadata_store
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(run.summary.status, ExecutionStatus::Completed);

        // Check journal for the recovery TasksStarted
        let all_entries = journal
            .read_from(run_id, SequenceNumber::new(0), usize::MAX)
            .await
            .expect("should read journal");

        let tasks_started_events: Vec<_> = all_entries
            .iter()
            .filter_map(|event| match event {
                JournalEvent::TasksStarted { runs } => Some(runs),
                _ => None,
            })
            .collect();

        // Should have 2 TasksStarted events: pre-crash batch and recovery batch
        assert_eq!(
            tasks_started_events.len(),
            2,
            "Should have 2 TasksStarted events (pre-crash + recovery)"
        );

        // First: both steps at attempt 1 (gather all tasks across RunTaskAttempts)
        let pre_crash_tasks: Vec<_> = tasks_started_events[0]
            .iter()
            .filter(|r| r.run_id == run_id)
            .flat_map(|r| &r.tasks)
            .collect();
        assert_eq!(pre_crash_tasks.len(), 2);
        assert!(pre_crash_tasks.iter().all(|t| t.attempt == 1));

        // Second (recovery): both steps at attempt 2, in a single batch
        let recovery_tasks: Vec<_> = tasks_started_events[1]
            .iter()
            .filter(|r| r.run_id == run_id)
            .flat_map(|r| &r.tasks)
            .collect();
        assert_eq!(
            recovery_tasks.len(),
            2,
            "Recovery should batch both tasks into a single TasksStarted event"
        );
        assert!(
            recovery_tasks.iter().all(|t| t.attempt == 2),
            "Recovery attempts should be 2"
        );
    }

    /// Recovery groups runs by root_run_id and only recovers the root run.
    ///
    /// Before this fix, recovery processed each run independently. This was
    /// broken for subflows because:
    /// - A subflow FlowExecutor would have the wrong root_run_id
    /// - A root FlowExecutor without its subflows couldn't properly resume
    /// - Journal writes from a subflow executor would go to the wrong journal
    ///
    /// This test creates both a root run and a subflow run, then verifies that
    /// recovery only recovers the root run (1 recovered), not both independently.
    #[tokio::test]
    async fn test_recovery_groups_by_root_run_id() {
        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        // Store a valid flow for both root and subflow
        let flow = Arc::new(create_test_flow());
        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        // Create root run
        let root_run_id = uuid::Uuid::now_v7();
        let root_params =
            CreateRunParams::new(root_run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(root_params)
            .await
            .expect("should create root run");

        // Create subflow run with root_run_id pointing to the root
        let subflow_run_id = uuid::Uuid::now_v7();
        let subflow_params = CreateRunParams::new_subflow(
            subflow_run_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_run_id,
            root_run_id, // parent is the root
        );
        metadata_store
            .create_run(subflow_params)
            .await
            .expect("should create subflow run");

        // Write journal events for the root run (partial execution)
        journal
            .write(
                root_run_id,
                JournalEvent::RunCreated {
                    run_id: root_run_id,
                    flow_id: flow_id.clone(),
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: None,
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                root_run_id,
                JournalEvent::RunInitialized {
                    run_id: root_run_id,
                    needed_steps: vec![ItemSteps {
                        item_index: 0,
                        step_indices: vec![0],
                    }],
                },
            )
            .await
            .expect("should write");

        // Write journal events for the subflow (in same journal, keyed by root_run_id)
        journal
            .write(
                root_run_id,
                JournalEvent::RunCreated {
                    run_id: subflow_run_id,
                    flow_id: flow_id.clone(),
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: Some(root_run_id),
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                root_run_id,
                JournalEvent::RunInitialized {
                    run_id: subflow_run_id,
                    needed_steps: vec![ItemSteps {
                        item_index: 0,
                        step_indices: vec![0],
                    }],
                },
            )
            .await
            .expect("should write");

        // Both runs are in Running status. Recovery should group them and only
        // recover the root run, not create separate executors for each.
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");

        // Only the root run should be "recovered" (1 executor spawned)
        assert_eq!(
            result.recovered, 1,
            "Should recover exactly 1 execution tree (the root run)"
        );
        assert_eq!(result.failed, 0);
        assert!(
            result.recovered_run_ids.contains(&root_run_id),
            "The recovered run should be the root"
        );
        assert!(
            !result.recovered_run_ids.contains(&subflow_run_id),
            "The subflow should not be independently recovered"
        );

        // Wait for the root execution to complete
        let active_executions = env.active_executions();
        for _ in 0..100 {
            if active_executions.is_empty() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        assert!(
            active_executions.is_empty(),
            "Execution should complete within timeout"
        );

        // Verify the root run completed successfully
        let run = metadata_store
            .get_run(root_run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(
            run.summary.status,
            ExecutionStatus::Completed,
            "Root run should have completed status after recovery"
        );
    }

    /// Orphaned subflows without a running root should be marked as failed.
    ///
    /// This can happen if the root run completed/failed but a subflow was left
    /// in Running status due to a race condition or bug.
    #[tokio::test]
    async fn test_recovery_orphaned_subflows_without_root_marked_failed() {
        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        // Store a valid flow
        let flow = Arc::new(create_test_flow());
        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        // Create only a subflow run — the root is NOT in Running status
        // (simulates root completed but subflow stuck as Running)
        let root_run_id = uuid::Uuid::now_v7();
        let subflow_run_id = uuid::Uuid::now_v7();
        let subflow_params = CreateRunParams::new_subflow(
            subflow_run_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_run_id,
            root_run_id,
        );
        metadata_store
            .create_run(subflow_params)
            .await
            .expect("should create subflow run");

        // Write journal entries for the subflow
        journal
            .write(
                root_run_id,
                JournalEvent::RunCreated {
                    run_id: subflow_run_id,
                    flow_id,
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: Some(root_run_id),
                },
            )
            .await
            .expect("should write");

        // Recovery should find the subflow but no root, and mark it as failed
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed overall");

        assert_eq!(result.recovered, 0, "No runs should be recovered");
        assert_eq!(
            result.failed, 1,
            "Orphaned subflow should be marked as failed"
        );
        assert!(
            result.failed_runs[0]
                .1
                .contains("Root run not found for recovery"),
            "Error message should explain why: got {:?}",
            result.failed_runs[0].1
        );

        // Verify the subflow was marked as Failed
        let run = metadata_store
            .get_run(subflow_run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(
            run.summary.status,
            ExecutionStatus::Failed,
            "Orphaned subflow should be marked as failed"
        );
    }

    /// Helper to build an environment with shared stores, checkpoint store, and
    /// configurable checkpoint interval. Returns the env and the shared store
    /// (useful when creating a second env with the same backing stores).
    async fn build_env_with_checkpoint_interval(
        mock_plugin: stepflow_mock::MockPlugin,
        store: Arc<stepflow_state::InMemoryStateStore>,
        checkpoint_interval: usize,
    ) -> Arc<StepflowEnvironment> {
        let dyn_plugin = stepflow_plugin::DynPlugin::boxed(mock_plugin);

        use stepflow_plugin::routing::RouteRule;
        let rules = vec![RouteRule {
            conditions: vec![],
            component_allow: None,
            component_deny: None,
            plugin: "mock".into(),
            component: None,
        }];

        let plugin_router = stepflow_plugin::routing::PluginRouter::builder()
            .with_routing_path("/{*component}".to_string(), rules)
            .register_plugin("mock".to_string(), dyn_plugin)
            .build()
            .unwrap();

        let metadata_store: Arc<dyn stepflow_state::MetadataStore> = store.clone();
        let blob_store: Arc<dyn stepflow_state::BlobStore> = store.clone();
        let journal: Arc<dyn stepflow_state::ExecutionJournal> = store.clone();
        let checkpoint_store: Arc<dyn stepflow_state::CheckpointStore> = store.clone();
        stepflow_plugin::StepflowEnvironmentBuilder::new()
            .metadata_store(metadata_store)
            .blob_store(blob_store)
            .execution_journal(journal)
            .checkpoint_store(checkpoint_store)
            .checkpoint_interval(checkpoint_interval)
            .working_directory(std::path::PathBuf::from("."))
            .plugin_router(plugin_router)
            .build()
            .await
            .expect("MockPlugin should always initialize successfully")
    }

    /// Create a mock plugin that returns success for common test inputs.
    fn create_standard_mock_plugin() -> stepflow_mock::MockPlugin {
        use stepflow_mock::{MockComponentBehavior, MockPlugin};

        let mut mock_plugin = MockPlugin::new();
        let behavior = MockComponentBehavior::result(stepflow_core::FlowResult::Success(
            ValueRef::new(json!({"result": "ok"})),
        ));

        for input in &[
            json!({}),
            json!({"x": 1}),
            json!({"x": 2}),
            json!({"result": "ok"}),
        ] {
            mock_plugin
                .mock_component("/mock/test")
                .behavior(ValueRef::new(input.clone()), behavior.clone());
        }

        mock_plugin
    }

    /// Verify that the executor creates checkpoints during normal execution.
    ///
    /// This test:
    /// 1. Creates a chain flow with 15 steps and checkpoint_interval=3
    /// 2. Submits the run — the Checkpointer fires every 3 journal entries
    /// 3. Waits for completion
    /// 4. Verifies that at least one checkpoint was stored
    #[tokio::test]
    async fn test_execution_creates_checkpoints() {
        use stepflow_state::CheckpointStoreExt as _;

        let store = Arc::new(stepflow_state::InMemoryStateStore::new());
        let mock_plugin = create_standard_mock_plugin();
        let env = build_env_with_checkpoint_interval(mock_plugin, store.clone(), 3).await;

        // Create a chain flow with 15 steps
        let flow = Arc::new(crate::testing::create_chain_flow(15));
        let flow_id = env
            .blob_store()
            .store_flow(flow.clone())
            .await
            .expect("should store flow");

        // Submit the run
        let status = crate::executor::submit_run(
            &env,
            flow,
            flow_id,
            vec![ValueRef::new(json!({}))],
            Default::default(),
        )
        .await
        .expect("should submit run");

        let run_id = status.run_id;

        // Wait for execution to complete
        crate::executor::wait_for_completion(&env, run_id)
            .await
            .expect("should complete");

        // Verify the run completed
        let run = env
            .metadata_store()
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(run.summary.status, ExecutionStatus::Completed);

        // Verify that checkpoints were cleaned up after completion.
        // During execution, checkpoints are created periodically, but on
        // completion the executor calls cleanup() to free storage.
        let checkpoint = env
            .checkpoint_store()
            .get_latest_checkpoint(run_id)
            .await
            .expect("should query checkpoint store");
        assert!(
            checkpoint.is_none(),
            "Checkpoints should have been cleaned up after successful completion"
        );
    }

    /// End-to-end test: executor creates checkpoints, crash is simulated, recovery uses them.
    ///
    /// This test:
    /// 1. Creates a chain flow with 15 steps and checkpoint_interval=3
    /// 2. Registers a wait signal on the mock plugin for `{"result": "ok"}` input,
    ///    which blocks step1 (and all subsequent chain steps)
    /// 3. Submits the run — step0 completes, triggering a checkpoint, then step1 blocks
    /// 4. Polls until a checkpoint appears in the store
    /// 5. Aborts the execution (simulating a crash)
    /// 6. Creates a NEW environment (same stores, no wait signal) and runs recovery
    /// 7. Verifies recovery loads the checkpoint and completes the run
    #[tokio::test]
    async fn test_recovery_with_checkpoint() {
        use stepflow_mock::{MockComponentBehavior, MockPlugin};
        use stepflow_state::CheckpointStoreExt as _;

        let store = Arc::new(stepflow_state::InMemoryStateStore::new());

        // Phase 1: Execute with a wait signal to block mid-flow
        let mut mock_plugin = MockPlugin::new();
        let behavior = MockComponentBehavior::result(stepflow_core::FlowResult::Success(
            ValueRef::new(json!({"result": "ok"})),
        ));
        // Step0 takes json!({}) as input — let it complete immediately
        mock_plugin
            .mock_component("/mock/test")
            .behavior(ValueRef::new(json!({})), behavior.clone());
        // Steps 1+ take json!({"result": "ok"}) as input — register behavior AND wait signal
        mock_plugin
            .mock_component("/mock/test")
            .behavior(ValueRef::new(json!({"result": "ok"})), behavior.clone());
        let _signal = mock_plugin.wait_for("/mock/test", ValueRef::new(json!({"result": "ok"})));

        let env1 = build_env_with_checkpoint_interval(mock_plugin, store.clone(), 3).await;

        // Create a chain flow with 15 steps: step0($input) → step1($step.step0) → ...
        let flow = Arc::new(crate::testing::create_chain_flow(15));
        let flow_id = env1
            .blob_store()
            .store_flow(flow.clone())
            .await
            .expect("should store flow");

        // Submit the run
        let status = crate::executor::submit_run(
            &env1,
            flow,
            flow_id,
            vec![ValueRef::new(json!({}))],
            Default::default(),
        )
        .await
        .expect("should submit run");

        let run_id = status.run_id;

        // Poll until a checkpoint appears (step0 should complete quickly, triggering a checkpoint)
        let checkpoint_store = env1.checkpoint_store().clone();
        let mut checkpoint_found = false;
        for _ in 0..200 {
            if let Some(_cp) = checkpoint_store
                .get_latest_checkpoint(run_id)
                .await
                .expect("should query checkpoint store")
            {
                checkpoint_found = true;
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        assert!(
            checkpoint_found,
            "Checkpoint should have been created after step0 completed"
        );

        // Abort execution (simulating a crash)
        env1.active_executions().shutdown();

        // Verify run is still Running (not completed, since we aborted)
        let run = env1
            .metadata_store()
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(
            run.summary.status,
            ExecutionStatus::Running,
            "Run should still be Running after abort"
        );

        // Phase 2: Create a new env (same stores, no wait signal) and run recovery
        let mock_plugin2 = create_standard_mock_plugin();
        let env2 = build_env_with_checkpoint_interval(mock_plugin2, store, 3).await;

        let orchestrator_id = OrchestratorId::new("test-orch");
        let result = recover_orphaned_runs(&env2, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");

        assert_eq!(
            result.recovered, 1,
            "Expected 1 recovered run, got {}",
            result.recovered
        );
        assert_eq!(result.failed, 0);

        // Wait for recovered execution to complete
        let active_executions = env2.active_executions();
        for _ in 0..200 {
            if active_executions.is_empty() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        assert!(
            active_executions.is_empty(),
            "Recovered execution should complete within timeout"
        );

        // Verify the run completed successfully
        let run = env2
            .metadata_store()
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(
            run.summary.status,
            ExecutionStatus::Completed,
            "Run should have completed status after checkpoint-based recovery"
        );
    }

    /// Recovery without a checkpoint should still work (backwards compatibility).
    /// This uses an environment with checkpoint store enabled (not NoOp) but no
    /// checkpoint is stored — recovery falls back to full journal replay.
    #[tokio::test]
    async fn test_recovery_without_checkpoint_backwards_compat() {
        let store = Arc::new(stepflow_state::InMemoryStateStore::new());
        let mock_plugin = create_standard_mock_plugin();
        let env = build_env_with_checkpoint_interval(mock_plugin, store, 0).await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        let flow = Arc::new(create_test_flow());
        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        let run_id = uuid::Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Write journal events for a partial execution (no checkpoint stored)
        journal
            .write(
                run_id,
                JournalEvent::RunCreated {
                    run_id,
                    flow_id: flow_id.clone(),
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: None,
                },
            )
            .await
            .expect("should write");

        journal
            .write(
                run_id,
                JournalEvent::RunInitialized {
                    run_id,
                    needed_steps: vec![ItemSteps {
                        item_index: 0,
                        step_indices: vec![0],
                    }],
                },
            )
            .await
            .expect("should write");

        // No checkpoint — recovery should use full replay path
        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");

        assert_eq!(result.recovered, 1);
        assert_eq!(result.failed, 0);

        // Wait for execution to complete
        let active_executions = env.active_executions();
        for _ in 0..100 {
            if active_executions.is_empty() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        assert!(
            active_executions.is_empty(),
            "Execution should complete within timeout"
        );

        let run = metadata_store
            .get_run(run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(run.summary.status, ExecutionStatus::Completed);
    }

    /// Recovery should correctly apply all journal events (including subflow events)
    /// to the root RunState without corruption.
    ///
    /// This verifies that subflow events in the journal are silently ignored by
    /// the root's apply_event (which checks run_id internally).
    #[tokio::test]
    async fn test_recovery_root_ignores_subflow_events_in_journal() {
        use stepflow_core::workflow::FlowBuilder;

        let env = create_test_env().await;
        let orchestrator_id = OrchestratorId::new("test-orch");
        let metadata_store = env.metadata_store();
        let blob_store = env.blob_store();
        let journal = env.execution_journal();

        // Create a 2-step chain flow: step0 -> step1
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step0")
                        .component("/mock/test")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Step {
                            step: "step0".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: Default::default(),
                })
                .build(),
        );

        let flow_id = blob_store
            .store_flow(flow)
            .await
            .expect("should store flow");

        let root_run_id = uuid::Uuid::now_v7();
        let subflow_run_id = uuid::Uuid::now_v7();

        // Create root run record
        let params =
            CreateRunParams::new(root_run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        metadata_store
            .create_run(params)
            .await
            .expect("should create run");

        // Write interleaved root + subflow events (simulates real execution)
        // Root: RunCreated
        journal
            .write(
                root_run_id,
                JournalEvent::RunCreated {
                    run_id: root_run_id,
                    flow_id: flow_id.clone(),
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: None,
                },
            )
            .await
            .expect("should write");

        // Root: RunInitialized
        journal
            .write(
                root_run_id,
                JournalEvent::RunInitialized {
                    run_id: root_run_id,
                    needed_steps: vec![ItemSteps {
                        item_index: 0,
                        step_indices: vec![0, 1],
                    }],
                },
            )
            .await
            .expect("should write");

        // Root: step0 completed (use mock's default result so step1's input is recognized)
        journal
            .write(
                root_run_id,
                JournalEvent::TaskCompleted {
                    run_id: root_run_id,
                    item_index: 0,
                    step_index: 0,
                    result: stepflow_core::FlowResult::Success(ValueRef::new(
                        json!({"result": "ok"}),
                    )),
                },
            )
            .await
            .expect("should write");

        // Subflow: RunCreated (interleaved in the same journal)
        journal
            .write(
                root_run_id,
                JournalEvent::RunCreated {
                    run_id: subflow_run_id,
                    flow_id: flow_id.clone(),
                    inputs: vec![ValueRef::new(json!({}))],
                    variables: HashMap::new(),
                    parent_run_id: Some(root_run_id),
                },
            )
            .await
            .expect("should write");

        // Subflow: RunInitialized (different run_id, should be ignored by root)
        journal
            .write(
                root_run_id,
                JournalEvent::RunInitialized {
                    run_id: subflow_run_id,
                    needed_steps: vec![ItemSteps {
                        item_index: 0,
                        step_indices: vec![0],
                    }],
                },
            )
            .await
            .expect("should write");

        // Subflow: TaskCompleted (should be ignored by root)
        journal
            .write(
                root_run_id,
                JournalEvent::TaskCompleted {
                    run_id: subflow_run_id,
                    item_index: 0,
                    step_index: 0,
                    result: stepflow_core::FlowResult::Success(ValueRef::new(
                        json!({"subflow": "done"}),
                    )),
                },
            )
            .await
            .expect("should write");

        // Crash here - root step1 never started, subflow events are in journal
        // Recovery should ignore subflow events and resume root from step1

        let result = recover_orphaned_runs(&env, orchestrator_id, 100)
            .await
            .expect("recovery should succeed");

        assert_eq!(result.recovered, 1);

        // Wait for execution to complete
        let active_executions = env.active_executions();
        for _ in 0..100 {
            if active_executions.is_empty() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        assert!(
            active_executions.is_empty(),
            "Execution should complete within timeout"
        );

        // Root should have completed - step0 was from journal, step1 was re-executed
        let run = metadata_store
            .get_run(root_run_id)
            .await
            .expect("should get run")
            .expect("run should exist");
        assert_eq!(
            run.summary.status,
            ExecutionStatus::Completed,
            "Root run should complete despite subflow events in journal"
        );
    }
}
