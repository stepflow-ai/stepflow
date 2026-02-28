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

//! Execution state restoration from journal replay and/or checkpoints.
//!
//! Both recovery paths (full journal replay and checkpoint-accelerated) produce
//! the same output: a [`RecoveredState`] containing the root run's state, a
//! subflow deduplication map, and any in-flight subflow states.
//!
//! The shared [`replay_events`] function handles the common tail-replay logic:
//! applying events to all RunStates, creating new subflow RunStates on
//! `SubRunCreated`, tracking in-flight subflows, and syncing the metadata store
//! on `RunCompleted` (handling the crash window between journal write and
//! metadata update).

use std::collections::HashMap;
use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_core::status::ExecutionStatus;
use stepflow_state::{ExecutionJournal, SequenceNumber};

use super::types::RecoveredState;
use crate::checkpoint::CheckpointData;
use crate::{ExecutionError, Result, RunState};

// ---------------------------------------------------------------------------
// Full journal replay (no checkpoint)
// ---------------------------------------------------------------------------

/// Full journal replay path — no checkpoint available.
///
/// Reads the entire journal from sequence 0, extracts `RunCreated` for the root
/// run, then replays all events through the shared [`replay_events`] function
/// to reconstruct execution state for both the root and any subflows.
///
/// For completed subflows, checks the metadata store to determine whether
/// results have been persisted. If the journal records a `RunCompleted` but
/// the metadata store still shows `Running` (crash between journal write and
/// metadata update), the function reconstructs the subflow's state and syncs
/// the metadata store.
pub(super) async fn restore_from_journal(
    journal: &Arc<dyn ExecutionJournal>,
    root_run_id: uuid::Uuid,
    run_id: uuid::Uuid,
    root_info: &stepflow_state::RunRecoveryInfo,
    flow: &Arc<stepflow_core::workflow::Flow>,
    blob_store: &dyn stepflow_state::BlobStore,
    metadata_store: &dyn stepflow_state::MetadataStore,
) -> Result<RecoveredState> {
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

    let mut subflow_runs: HashMap<uuid::Uuid, RunState> = HashMap::new();
    let mut subflow_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid> =
        HashMap::new();
    let mut inflight_subflow_run_ids: std::collections::HashSet<uuid::Uuid> =
        std::collections::HashSet::new();

    // Full replay from beginning — no useful lower bound for pruning.
    replay_events(
        &all_events,
        run_id,
        root_run_id,
        &mut run_state,
        &mut subflow_runs,
        &mut subflow_map,
        &mut inflight_subflow_run_ids,
        blob_store,
        metadata_store,
        || ExecutionError::RecoveryFailed,
        None,
    )
    .await?;

    log::info!(
        "Replayed {} journal events for execution tree {}, root complete={}",
        all_events.len(),
        root_run_id,
        run_state.is_complete()
    );

    Ok(RecoveredState {
        run_state,
        subflow_map,
        subflow_runs,
        inflight_subflow_run_ids,
    })
}

// ---------------------------------------------------------------------------
// Checkpoint-accelerated recovery
// ---------------------------------------------------------------------------

/// Restore execution state from a checkpoint.
///
/// Deserializes the checkpoint, validates structural integrity, rebuilds all
/// RunStates and the subflow dedup map, then replays tail journal events
/// through the shared [`replay_events`] function.
///
/// When a `RunCompleted` tail event is encountered for a subflow, the metadata
/// store is checked. If it is out of sync (crash between journal write and
/// metadata update), the subflow's item results and status are synced before
/// evicting the RunState.
///
/// Returns an error if the checkpoint is present but cannot be restored
/// (deserialization failure, structural mismatch, missing blobs). The caller
/// should treat this as a hard failure — the full journal may not be available
/// after a checkpoint has been written.
pub(super) async fn restore_from_checkpoint(
    stored_cp: &stepflow_state::StoredCheckpoint,
    run_id: uuid::Uuid,
    root_run_id: uuid::Uuid,
    flow: &std::sync::Arc<stepflow_core::workflow::Flow>,
    blob_store: &dyn stepflow_state::BlobStore,
    journal: &dyn ExecutionJournal,
    metadata_store: &dyn stepflow_state::MetadataStore,
) -> Result<RecoveredState> {
    let checkpoint_data = CheckpointData::deserialize(&stored_cp.data).map_err(|e| {
        error_stack::report!(ExecutionError::CheckpointError)
            .attach_printable(format!("checkpoint deserialization failed: {e}"))
    })?;

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
            error_stack::report!(ExecutionError::CheckpointError)
                .attach_printable("Root run not found in checkpoint")
        })?;

    let mut run_state = RunState::from_checkpoint(root_checkpoint, flow.clone()).map_err(|e| {
        error_stack::report!(ExecutionError::CheckpointError)
            .attach_printable(format!("root run restore failed: {e}"))
    })?;

    // Restore subflow RunStates from checkpoint
    let mut subflow_runs: HashMap<uuid::Uuid, RunState> = HashMap::new();
    for rc in &checkpoint_data.runs {
        if rc.run_id == run_id {
            continue; // Skip root, already restored
        }
        let sub_flow = blob_store
            .get_flow(&rc.flow_id)
            .await
            .change_context(ExecutionError::CheckpointError)?
            .ok_or_else(|| {
                error_stack::report!(ExecutionError::CheckpointError).attach_printable(format!(
                    "Subflow flow not found for run {}, flow_id={}",
                    rc.run_id, rc.flow_id
                ))
            })?;
        let sub_state = RunState::from_checkpoint(rc, sub_flow).map_err(|e| {
            error_stack::report!(ExecutionError::CheckpointError)
                .attach_printable(format!("subflow {} restore failed: {e}", rc.run_id))
        })?;
        subflow_runs.insert(rc.run_id, sub_state);
    }

    // Restore subflow dedup map from checkpoint
    let mut subflow_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid> =
        HashMap::new();
    for mapping in &checkpoint_data.subflow_map {
        subflow_map.insert(
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
    let tail_events = journal
        .read_from(root_run_id, stored_cp.sequence.next(), usize::MAX)
        .await
        .change_context(ExecutionError::CheckpointError)?;

    // Seed in-flight set: all checkpoint-restored subflows were past initialization.
    let mut inflight_subflow_run_ids: std::collections::HashSet<uuid::Uuid> = checkpoint_data
        .runs
        .iter()
        .filter(|rc| rc.run_id != run_id)
        .map(|rc| rc.run_id)
        .collect();

    // Checkpoint recovery: use the checkpoint sequence as a lower bound to
    // prune sub-runs created before the checkpoint from the metadata query.
    replay_events(
        &tail_events,
        run_id,
        root_run_id,
        &mut run_state,
        &mut subflow_runs,
        &mut subflow_map,
        &mut inflight_subflow_run_ids,
        blob_store,
        metadata_store,
        || ExecutionError::CheckpointError,
        Some(stored_cp.sequence),
    )
    .await?;

    log::info!(
        "Restored from checkpoint + replayed {} tail events for tree {}, root complete={}",
        tail_events.len(),
        root_run_id,
        run_state.is_complete()
    );

    Ok(RecoveredState {
        run_state,
        subflow_map,
        subflow_runs,
        inflight_subflow_run_ids,
    })
}

// ---------------------------------------------------------------------------
// Shared replay logic
// ---------------------------------------------------------------------------

/// Replay a sequence of journal events, applying them to all in-memory
/// RunStates and handling subflow lifecycle events.
///
/// This is the core replay loop shared by both journal-only and checkpoint
/// recovery paths. For each event it:
///
/// 1. Applies the event to the root RunState and all subflow RunStates.
/// 2. On `SubRunCreated`: updates the dedup map and creates a new RunState
///    (loading the flow definition from the blob store).
/// 3. On `RunInitialized` (non-root): adds the run to the in-flight set.
/// 4. On `RunCompleted` (non-root): syncs the metadata store if needed
///    (crash window recovery), then evicts the RunState.
///
/// Before iterating, pre-fetches all completed/failed sub-runs from the
/// metadata store in a single batch query. For checkpoint recovery,
/// `replay_start_offset` prunes the query to sub-runs created at or after
/// the checkpoint sequence, avoiding a full scan of the tree.
#[allow(clippy::too_many_arguments)]
async fn replay_events(
    events: &[stepflow_state::JournalEvent],
    run_id: uuid::Uuid,
    root_run_id: uuid::Uuid,
    run_state: &mut RunState,
    subflow_runs: &mut HashMap<uuid::Uuid, RunState>,
    subflow_map: &mut HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid>,
    inflight_subflow_run_ids: &mut std::collections::HashSet<uuid::Uuid>,
    blob_store: &dyn stepflow_state::BlobStore,
    metadata_store: &dyn stepflow_state::MetadataStore,
    make_error: fn() -> ExecutionError,
    replay_start_offset: Option<SequenceNumber>,
) -> Result<()> {
    // Pre-fetch completed sub-runs from the metadata store in a single query.
    // Uses `replay_start_offset` as a `created_at_seqno` lower bound to
    // prune sub-runs created before the replay window.
    let completed_in_metadata = prefetch_completed_subflows(
        run_id, root_run_id, metadata_store, make_error, replay_start_offset,
    )
    .await?;

    for event in events {
        // Apply event to root and all subflow RunStates
        run_state.apply_event(event);
        for sub_state in subflow_runs.values_mut() {
            sub_state.apply_event(event);
        }

        // Handle SubRunCreated: update dedup map and create RunState.
        // The new sub-run is inserted into subflow_runs so subsequent events
        // in this loop are applied to it naturally.
        if let stepflow_state::JournalEvent::SubRunCreated {
            run_id: sub_run_id,
            flow_id: sub_flow_id,
            inputs: sub_inputs,
            variables: sub_variables,
            parent_run_id,
            item_index,
            step_index,
            subflow_key,
        } = event
        {
            subflow_map.insert(
                (*parent_run_id, *item_index, *step_index, *subflow_key),
                *sub_run_id,
            );
            if !subflow_runs.contains_key(sub_run_id) {
                let sub_flow = blob_store
                    .get_flow(sub_flow_id)
                    .await
                    .change_context(make_error())?
                    .ok_or_else(|| {
                        error_stack::report!(make_error()).attach_printable(format!(
                            "Subflow flow not found for run {sub_run_id}, flow_id={sub_flow_id}"
                        ))
                    })?;
                let sub_state = RunState::new_subflow(
                    *sub_run_id,
                    sub_flow_id.clone(),
                    root_run_id,
                    *parent_run_id,
                    sub_flow,
                    sub_inputs.clone(),
                    sub_variables.clone(),
                );
                subflow_runs.insert(*sub_run_id, sub_state);
            }
        }

        // Track in-flight subflows: add on RunInitialized (non-root).
        if let stepflow_state::JournalEvent::RunInitialized { run_id: rid, .. } = event
            && *rid != run_id
        {
            inflight_subflow_run_ids.insert(*rid);
        }

        // Handle RunCompleted for subflows: sync metadata if needed, then evict.
        if let stepflow_state::JournalEvent::RunCompleted {
            run_id: completed_id,
            status,
        } = event
            && *completed_id != run_id
        {
            // If we have a RunState for this subflow and it's not already synced
            // in the metadata store, sync it (crash window: journal RunCompleted
            // written but metadata store not updated before crash).
            if let Some(sub_state) = subflow_runs.get(completed_id)
                && !completed_in_metadata.contains(completed_id)
            {
                log::info!(
                    "Syncing metadata for completed subflow {} (status={:?})",
                    completed_id,
                    status
                );
                sync_run_state_to_metadata(
                    *completed_id,
                    sub_state,
                    *status,
                    metadata_store,
                    make_error,
                )
                .await?;
            }

            subflow_runs.remove(completed_id);
            inflight_subflow_run_ids.remove(completed_id);
        }
    }

    Ok(())
}

/// Pre-fetch sub-runs that already have terminal status in the metadata store.
///
/// Queries the metadata store for all sub-runs in this execution tree, optionally
/// pruned by `replay_start_offset` (the journal sequence number at which replay
/// begins). For checkpoint recovery, this prunes sub-runs created before the
/// checkpoint; for full replay, `None` queries all sub-runs.
///
/// Sub-runs created before the checkpoint that complete in the tail events may
/// not be in this set. That's acceptable — sync is idempotent, so a redundant
/// sync for these edge cases is harmless.
///
/// Returns a set of sub-run IDs whose metadata is already in sync (terminal
/// status), so the caller can skip per-event metadata store checks.
async fn prefetch_completed_subflows(
    run_id: uuid::Uuid,
    root_run_id: uuid::Uuid,
    metadata_store: &dyn stepflow_state::MetadataStore,
    make_error: fn() -> ExecutionError,
    replay_start_offset: Option<SequenceNumber>,
) -> Result<std::collections::HashSet<uuid::Uuid>> {
    let filters = stepflow_dtos::RunFilters {
        root_run_id: Some(root_run_id),
        created_at_seqno_gte: replay_start_offset.map(|s| s.value()),
        ..Default::default()
    };

    let runs = metadata_store
        .list_runs(&filters)
        .await
        .change_context(make_error())?;

    Ok(runs
        .into_iter()
        .filter(|r| {
            r.run_id != run_id
                && matches!(
                    r.status,
                    ExecutionStatus::Completed | ExecutionStatus::Failed
                )
        })
        .map(|r| r.run_id)
        .collect())
}

/// Write a completed subflow's item results and status to the metadata store.
///
/// This resolves each item's output from the RunState and records it,
/// then updates the run status to match the journal.
async fn sync_run_state_to_metadata(
    run_id: uuid::Uuid,
    sub_state: &RunState,
    status: ExecutionStatus,
    metadata_store: &dyn stepflow_state::MetadataStore,
    make_error: fn() -> ExecutionError,
) -> Result<()> {
    let items_state = sub_state.items_state();
    for item_index in 0..items_state.item_count() {
        let item = items_state.item(item_index);
        let result = sub_state.flow().output().resolve(item);
        let step_statuses = items_state.get_item_step_statuses(item_index);
        metadata_store
            .record_item_result(run_id, item_index as usize, result, step_statuses)
            .await
            .change_context(make_error())?;
    }
    metadata_store
        .update_run_status(run_id, status)
        .await
        .change_context(make_error())?;
    Ok(())
}
