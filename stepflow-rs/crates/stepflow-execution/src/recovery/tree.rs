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

//! Execution tree recovery via journal replay.
//!
//! Given a root run, loads its journal, reconstructs all RunStates (root +
//! subflows), and spawns a new `FlowExecutor` to resume execution.

use std::collections::HashMap;
use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_core::workflow::apply_overrides;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    ActiveExecutionsExt as _, BlobStoreExt as _, CheckpointStoreExt as _, ExecutionJournal,
    MetadataStoreExt as _, SequenceNumber,
};

use super::checkpoint_restore::restore_from_checkpoint;
use crate::{ExecutionError, Result, RunState};

/// Information about a subflow discovered during journal replay.
struct SubflowInfo {
    flow_id: stepflow_core::BlobId,
    parent_id: uuid::Uuid,
    inputs: Vec<stepflow_core::workflow::ValueRef>,
    variables: HashMap<String, stepflow_core::workflow::ValueRef>,
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
pub(super) async fn recover_execution_tree(
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
    // If a checkpoint exists, restore state from it and only replay journal
    // events after the checkpoint sequence. Checkpoint restoration failures are
    // hard errors — once a checkpoint is written, the full journal may no longer
    // be available for replay.
    //
    // If no checkpoint exists, fall back to full replay from sequence 0.

    let checkpoint_store = env.checkpoint_store();
    let stored_checkpoint = checkpoint_store
        .get_latest_checkpoint(root_run_id)
        .await
        .change_context(ExecutionError::RecoveryFailed)?;

    let (run_state, subflow_map, additional_runs) = if let Some(ref stored_cp) = stored_checkpoint {
        // Checkpoint exists — restore from it. Errors are propagated as hard
        // failures because the full journal may have been truncated.
        restore_from_checkpoint(
            stored_cp,
            run_id,
            root_run_id,
            &flow,
            blob_store.as_ref(),
            journal.as_ref(),
        )
        .await
        .change_context(ExecutionError::RecoveryFailed)?
    } else {
        // ---- Full replay path (no checkpoint) ----
        full_journal_replay(journal, root_run_id, run_id, root_info, &flow, blob_store.as_ref())
            .await?
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

/// Full journal replay path — no checkpoint available.
///
/// Reads the entire journal from sequence 0, extracts RunCreated for root + subflows,
/// and applies all events to reconstruct execution state.
async fn full_journal_replay(
    journal: &Arc<dyn ExecutionJournal>,
    root_run_id: uuid::Uuid,
    run_id: uuid::Uuid,
    root_info: &stepflow_state::RunRecoveryInfo,
    flow: &Arc<stepflow_core::workflow::Flow>,
    blob_store: &dyn stepflow_state::BlobStore,
) -> Result<(
    RunState,
    HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid>,
    HashMap<uuid::Uuid, RunState>,
)> {
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

    Ok((run_state, sf_map, additional))
}
