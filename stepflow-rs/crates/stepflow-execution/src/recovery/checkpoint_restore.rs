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

//! Checkpoint-accelerated recovery.
//!
//! Restores execution state from a serialized checkpoint and replays only
//! the tail of the journal (events written after the checkpoint).

use std::collections::HashMap;

use error_stack::ResultExt as _;
use stepflow_state::ExecutionJournal;

use super::types::RecoveredState;
use crate::checkpoint::CheckpointData;
use crate::{ExecutionError, Result, RunState};

/// Restore execution state from a checkpoint.
///
/// Deserializes the checkpoint, validates structural integrity, rebuilds all
/// RunStates and the subflow dedup map, then replays tail journal events.
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

    let root_run_state = RunState::from_checkpoint(root_checkpoint, flow.clone()).map_err(|e| {
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
    let mut subflow_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid> = HashMap::new();
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
    let tail_events_for_replay = journal
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

    let mut run_state = root_run_state;
    for event in &tail_events_for_replay {
        run_state.apply_event(event);
        for sub_state in subflow_runs.values_mut() {
            sub_state.apply_event(event);
        }
        // Handle SubRunCreated: update dedup map and create RunState atomically.
        // The new sub-run is inserted into subflow_runs so the outer loop applies
        // all subsequent tail events to it naturally — no separate replay needed.
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
                    .change_context(ExecutionError::CheckpointError)?
                    .ok_or_else(|| {
                        error_stack::report!(ExecutionError::CheckpointError)
                            .attach_printable(format!(
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
        // Track in-flight subflows: add on RunInitialized, remove on RunCompleted.
        if let stepflow_state::JournalEvent::RunInitialized { run_id: rid, .. } = event
            && *rid != run_id
        {
            inflight_subflow_run_ids.insert(*rid);
        }
        // Handle RunCompleted events: evict completed subflows from in-memory state.
        // Their results are already in the metadata store.
        if let stepflow_state::JournalEvent::RunCompleted {
            run_id: completed_id,
            ..
        } = event
            && *completed_id != run_id
        {
            subflow_runs.remove(completed_id);
            inflight_subflow_run_ids.remove(completed_id);
        }
    }

    log::info!(
        "Restored from checkpoint + replayed {} tail events for tree {}, root complete={}",
        tail_events_for_replay.len(),
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
