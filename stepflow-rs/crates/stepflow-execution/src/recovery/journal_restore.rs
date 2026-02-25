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

//! Full journal replay recovery.
//!
//! Reads the entire journal from sequence 0 and reconstructs execution state
//! when no checkpoint is available.

use std::collections::HashMap;
use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_state::{ExecutionJournal, SequenceNumber};

use super::types::RecoveredState;
use crate::{ExecutionError, Result, RunState};

/// Information about a subflow discovered during journal replay.
struct SubflowInfo {
    flow_id: stepflow_core::BlobId,
    parent_id: uuid::Uuid,
    inputs: Vec<stepflow_core::workflow::ValueRef>,
    variables: HashMap<String, stepflow_core::workflow::ValueRef>,
}

/// Full journal replay path — no checkpoint available.
///
/// Reads the entire journal from sequence 0, extracts RunCreated for root + subflows,
/// and applies all events to reconstruct execution state.
pub(super) async fn restore_from_journal(
    journal: &Arc<dyn ExecutionJournal>,
    root_run_id: uuid::Uuid,
    run_id: uuid::Uuid,
    root_info: &stepflow_state::RunRecoveryInfo,
    flow: &Arc<stepflow_core::workflow::Flow>,
    blob_store: &dyn stepflow_state::BlobStore,
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
    let mut subflow_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid> = HashMap::new();
    for event in &all_events {
        if let stepflow_state::JournalEvent::SubflowSubmitted {
            parent_run_id,
            item_index,
            step_index,
            subflow_key,
            subflow_run_id,
        } = event
        {
            subflow_map.insert(
                (*parent_run_id, *item_index, *step_index, *subflow_key),
                *subflow_run_id,
            );
        }
    }

    // Find subflow RunCreated events and collect RunCompleted events.
    let mut subflow_created: HashMap<uuid::Uuid, SubflowInfo> = HashMap::new();
    let mut completed_runs: std::collections::HashSet<uuid::Uuid> =
        std::collections::HashSet::new();
    for event in &all_events {
        match event {
            stepflow_state::JournalEvent::RunCreated {
                run_id: sub_run_id,
                flow_id: sub_flow_id,
                inputs: sub_inputs,
                variables: sub_variables,
                parent_run_id: Some(parent_id),
            } if subflow_map.values().any(|id| id == sub_run_id) => {
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
            stepflow_state::JournalEvent::RunCompleted {
                run_id: completed_id,
                ..
            } => {
                completed_runs.insert(*completed_id);
            }
            _ => {}
        }
    }

    // For each *incomplete* subflow, reconstruct its RunState.
    // Completed subflows don't need RunState — their results are already in the
    // metadata store. The subflow_map (dedup map) still contains entries for
    // completed subflows so recovered_subflows.remove() returns the old run_id,
    // and wait_for_completion resolves immediately from the metadata store.
    let mut subflow_runs: HashMap<uuid::Uuid, RunState> = HashMap::new();
    for (sub_run_id, info) in &subflow_created {
        if completed_runs.contains(sub_run_id) {
            log::info!(
                "Skipping completed subflow {} during recovery (results in metadata store)",
                sub_run_id
            );
            continue;
        }
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

        subflow_runs.insert(*sub_run_id, sub_state);
    }

    Ok(RecoveredState {
        run_state,
        subflow_map,
        subflow_runs,
    })
}
