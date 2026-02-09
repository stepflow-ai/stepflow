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
//! 1. Querying the lease manager for runs that need recovery
//! 2. For each run, loading the flow definition from the metadata store
//! 3. Replaying journal events to reconstruct the execution state
//! 4. Resuming execution from where it left off
//!
//! ## Journal Organization
//!
//! Journals are keyed by `root_run_id`, meaning all events for an execution tree
//! (parent flow + all subflows) are stored in a single journal. During recovery,
//! we load the entire journal and filter events by `run_id` for each specific run.
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

use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    ActiveExecutionsExt as _, BlobStoreExt as _, ExecutionJournal, ExecutionJournalExt as _,
    LeaseManagerExt as _, MetadataStore, MetadataStoreExt as _, OrchestratorId, SequenceNumber,
};

use crate::{ExecutionError, Result, RunState};
use stepflow_core::workflow::apply_overrides;

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
/// If no lease manager is configured (single-orchestrator mode), returns
/// an empty result immediately since there's no distributed coordination.
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
    // Get lease manager from environment - if not configured, no recovery needed
    let Some(lease_manager) = env.lease_manager() else {
        log::debug!("No lease manager configured, skipping recovery");
        return Ok(RecoveryResult::new());
    };

    let state_store = env.metadata_store();
    let journal = env.execution_journal();

    // Claim runs for recovery
    let runs_to_recover = lease_manager
        .claim_for_recovery(
            orchestrator_id.clone(),
            &(state_store.clone() as Arc<dyn MetadataStore>),
            journal,
            limit,
        )
        .await
        .change_context(ExecutionError::RecoveryFailed)?;

    if runs_to_recover.is_empty() {
        log::info!("No runs to recover");
        return Ok(RecoveryResult::new());
    }

    log::info!("Found {} runs to recover", runs_to_recover.len());

    let mut result = RecoveryResult::new();

    for recovery_info in runs_to_recover {
        let run_id = recovery_info.run_id;
        log::info!("Recovering run {}", run_id);

        match recover_single_run(env, journal, &recovery_info).await {
            Ok(()) => {
                log::info!("Successfully recovered and resumed run {}", run_id);
                result.record_success(run_id);
            }
            Err(e) => {
                log::error!("Failed to recover run {}: {:?}", run_id, e);
                result.record_failure(run_id, format!("{:?}", e));
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

/// Recover a single run by replaying its journal and resuming execution.
async fn recover_single_run(
    env: &Arc<StepflowEnvironment>,
    journal: &Arc<dyn ExecutionJournal>,
    recovery_info: &stepflow_state::RunRecoveryInfo,
) -> Result<()> {
    let run_id = recovery_info.run_id;
    let root_run_id = recovery_info.root_run_id;
    let state_store = env.metadata_store();
    let blob_store = env.blob_store();

    // Load the flow definition
    let flow = blob_store
        .get_flow(&recovery_info.flow_id)
        .await
        .change_context(ExecutionError::RecoveryFailed)?
        .ok_or_else(|| {
            error_stack::report!(ExecutionError::RecoveryFailed)
                .attach_printable(format!("Flow not found: {}", recovery_info.flow_id))
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

    // Read journal entries for the root run's journal.
    // The journal is keyed by root_run_id and contains events for all runs in the tree.
    let all_entries = journal
        .read_from(root_run_id, SequenceNumber::new(0), usize::MAX)
        .await
        .change_context(ExecutionError::RecoveryFailed)?;

    // Filter entries to only those belonging to this specific run
    let run_entries: Vec<_> = all_entries
        .into_iter()
        .filter(|(_, entry)| entry.run_id == run_id)
        .collect();

    if run_entries.is_empty() {
        log::warn!(
            "No journal entries for run {} in root journal {}, cannot recover",
            run_id,
            root_run_id
        );
        return Err(error_stack::report!(ExecutionError::RecoveryFailed)
            .attach_printable("No journal entries found for this run"));
    }

    // Create RunState and apply events to reconstruct state
    // This validates that the journal can be replayed and counts ready tasks
    let mut run_state = if let Some(parent_run_id) = recovery_info.parent_run_id {
        // Subflow
        RunState::new_subflow(
            run_id,
            recovery_info.flow_id.clone(),
            recovery_info.root_run_id,
            parent_run_id,
            flow.clone(),
            recovery_info.inputs.clone(),
            recovery_info.variables.clone(),
        )
    } else {
        // Top-level run
        RunState::new(
            run_id,
            recovery_info.flow_id.clone(),
            flow.clone(),
            recovery_info.inputs.clone(),
            recovery_info.variables.clone(),
        )
    };

    // Apply events to reconstruct state
    run_entries.iter().for_each(|(_, entry)| {
        run_state.apply_event(&entry.event);
    });

    log::info!(
        "Replayed {} journal events for run {}, complete={}",
        run_entries.len(),
        run_id,
        run_state.is_complete()
    );

    // If the run is already complete from the journal, no need to resume
    if run_state.is_complete() {
        log::info!("Run {} already complete after journal replay", run_id);
        return Ok(());
    }

    // Build the FlowExecutor with the recovered state
    let flow_executor = crate::FlowExecutorBuilder::new(env.clone(), run_state)
        .skip_validation() // Already validated before crash
        .scheduler(Box::new(crate::DepthFirstScheduler::new()))
        .build()
        .await
        .change_context(ExecutionError::RecoveryFailed)?;

    // Spawn and track the recovered run
    flow_executor.spawn(env.active_executions());

    Ok(())
}
