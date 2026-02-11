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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use error_stack::ResultExt as _;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::apply_overrides;
use stepflow_dtos::RunFilters;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    ActiveExecutionsExt as _, BlobStoreExt as _, ExecutionJournal, ExecutionJournalExt as _,
    LeaseManagerExt as _, LeaseResult, MetadataStoreExt as _, OrchestratorId, RunRecoveryInfo,
    SequenceNumber,
};

/// Default TTL for leases acquired during recovery.
///
/// Recovery leases should be long enough to complete journal replay and resume
/// execution, after which the normal lease renewal mechanism takes over.
const RECOVERY_LEASE_TTL: Duration = Duration::from_secs(60);

use crate::{ExecutionError, Result, RunState};

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

                // Mark the run as Failed so it won't be retried on subsequent recovery attempts.
                // This makes recovery idempotent - unrecoverable runs (missing journal, corrupt
                // events, missing flow) are marked as failed rather than retried indefinitely.
                if let Err(update_err) = metadata_store
                    .update_run_status(run_id, ExecutionStatus::Failed)
                    .await
                {
                    log::error!(
                        "Failed to mark run {} as failed after recovery error: {:?}",
                        run_id,
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
            .acquire_lease(summary.run_id, orchestrator_id.clone(), RECOVERY_LEASE_TTL)
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

    // Extract inputs and variables from the RunCreated event in the journal.
    // This is the authoritative source - RunRecoveryInfo may have incomplete data.
    let (inputs, variables, parent_run_id) = run_entries
        .iter()
        .find_map(|(_, entry)| match &entry.event {
            stepflow_state::JournalEvent::RunCreated {
                inputs,
                variables,
                parent_run_id,
                ..
            } => Some((inputs.clone(), variables.clone(), *parent_run_id)),
            _ => None,
        })
        .ok_or_else(|| {
            error_stack::report!(ExecutionError::RecoveryFailed)
                .attach_printable("No RunCreated event found in journal")
        })?;

    // Create RunState and apply events to reconstruct state
    // This validates that the journal can be replayed and counts ready tasks
    let mut run_state = if let Some(parent_run_id) = parent_run_id {
        // Subflow
        RunState::new_subflow(
            run_id,
            recovery_info.flow_id.clone(),
            recovery_info.root_run_id,
            parent_run_id,
            flow.clone(),
            inputs,
            variables,
        )
    } else {
        // Top-level run
        RunState::new(
            run_id,
            recovery_info.flow_id.clone(),
            flow.clone(),
            inputs,
            variables,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;

    use stepflow_core::workflow::{FlowBuilder, StepBuilder, ValueRef};
    use stepflow_core::{BlobId, ValueExpr};
    use stepflow_state::{CreateRunParams, ItemSteps, JournalEntry, JournalEvent};

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
        let entry = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunCreated {
                flow_id: fake_flow_id,
                inputs: vec![ValueRef::new(json!({}))],
                variables: HashMap::new(),
                parent_run_id: None,
            },
        );
        journal.append(entry).await.expect("should append");

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
        let entry = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: stepflow_core::FlowResult::Success(ValueRef::new(json!({}))),
            },
        );
        journal.append(entry).await.expect("should append");

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
        let entry1 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunCreated {
                flow_id: flow_id.clone(),
                inputs: vec![ValueRef::new(json!({}))],
                variables: HashMap::new(),
                parent_run_id: None,
            },
        );
        journal.append(entry1).await.expect("should append");

        let entry2 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunInitialized {
                needed_steps: vec![ItemSteps {
                    item_index: 0,
                    step_indices: vec![0],
                }],
            },
        );
        journal.append(entry2).await.expect("should append");

        let entry3 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: stepflow_core::FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            },
        );
        journal.append(entry3).await.expect("should append");

        let entry4 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::ItemCompleted {
                item_index: 0,
                result: stepflow_core::FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            },
        );
        journal.append(entry4).await.expect("should append");

        let entry5 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunCompleted {
                status: ExecutionStatus::Completed,
            },
        );
        journal.append(entry5).await.expect("should append");

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

        let entry1 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunCreated {
                flow_id: flow_id.clone(),
                inputs: vec![ValueRef::new(json!({}))],
                variables: HashMap::new(),
                parent_run_id: None,
            },
        );
        journal.append(entry1).await.expect("should append");

        let entry2 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunInitialized {
                needed_steps: vec![ItemSteps {
                    item_index: 0,
                    step_indices: vec![0, 1], // Both steps needed
                }],
            },
        );
        journal.append(entry2).await.expect("should append");

        // Step0 completed successfully
        let step0_result = ValueRef::new(json!({"result": "ok"}));
        let entry3 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: stepflow_core::FlowResult::Success(step0_result),
            },
        );
        journal.append(entry3).await.expect("should append");

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
}
