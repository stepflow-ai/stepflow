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

use crate::checkpoint::CheckpointData;
use crate::{ExecutionError, Result, RunState};

/// The result of a successful checkpoint restoration: root RunState, subflow dedup
/// map, and additional (subflow) RunStates.
pub(super) type CheckpointRestoreResult = (
    RunState,
    HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid>,
    HashMap<uuid::Uuid, RunState>,
);

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
) -> Result<CheckpointRestoreResult> {
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
    let mut additional: HashMap<uuid::Uuid, RunState> = HashMap::new();
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
        .change_context(ExecutionError::CheckpointError)?;

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
                .change_context(ExecutionError::CheckpointError)?
                .ok_or_else(|| {
                    error_stack::report!(ExecutionError::CheckpointError).attach_printable(format!(
                        "Subflow flow not found for run {sub_run_id}, flow_id={sub_flow_id}"
                    ))
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

    Ok((root_state, sf_map, additional))
}
