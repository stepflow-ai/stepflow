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

use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_core::workflow::apply_overrides;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    ActiveExecutionsExt as _, BlobStoreExt as _, CheckpointStoreExt as _, ExecutionJournal,
    MetadataStoreExt as _,
};

use super::checkpoint_restore::restore_from_checkpoint;
use super::journal_restore::restore_from_journal;
use crate::{ExecutionError, Result};

/// Recover an execution tree by replaying its journal and resuming the root run.
///
/// This loads the full journal for the execution tree (keyed by `root_run_id`),
/// reconstructs the root run's state and any in-flight subflows, then spawns a
/// new `FlowExecutor` to resume execution.
///
/// ## Subflow Recovery
///
/// When `SubflowCreated` events are present in the journal, this function
/// reconstructs subflow `RunState` objects and builds a deduplication map.
/// When parent steps re-execute and re-submit subflows with the same deterministic
/// key, the executor matches against the recovered subflow and returns the existing
/// `run_id` instead of creating a duplicate. This avoids restarting completed or
/// in-progress subflows from scratch.
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

    let recovered = if let Some(ref stored_cp) = stored_checkpoint {
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
        // Full replay path (no checkpoint)
        restore_from_journal(
            journal,
            root_run_id,
            run_id,
            root_info,
            &flow,
            blob_store.as_ref(),
        )
        .await?
    };

    // If the run is already complete, no need to resume
    if recovered.run_state.is_complete() {
        log::info!("Root run {} already complete after recovery", run_id);
        return Ok(());
    }

    if !recovered.subflow_map.is_empty() {
        log::info!(
            "Recovered {} subflow mappings, {} subflow RunStates for tree {}",
            recovered.subflow_map.len(),
            recovered.subflow_runs.len(),
            root_run_id
        );
    }

    // Build the FlowExecutor with the recovered root state and subflow states.
    let mut builder = crate::FlowExecutorBuilder::new(env.clone(), recovered.run_state)
        .skip_validation() // Already validated before crash
        .scheduler(Box::new(crate::DepthFirstScheduler::new()));

    if !recovered.subflow_runs.is_empty() || !recovered.subflow_map.is_empty() {
        builder = builder.with_recovered_subflows(
            recovered.subflow_runs,
            recovered.subflow_map,
            recovered.inflight_subflow_run_ids,
        );
    }

    let flow_executor = builder
        .build()
        .await
        .change_context(ExecutionError::RecoveryFailed)?;

    // Spawn and track the recovered execution tree
    flow_executor.spawn(env.active_executions());

    Ok(())
}
