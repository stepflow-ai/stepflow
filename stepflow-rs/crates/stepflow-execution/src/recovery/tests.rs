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

use super::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::{FlowBuilder, StepBuilder, ValueRef};
use stepflow_core::{BlobId, ValueExpr};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    BlobStoreExt as _, CreateRunParams, ExecutionJournalExt as _, ItemSteps, JournalEvent,
    MetadataStoreExt as _, OrchestratorId, RunTaskAttempts, SequenceNumber,
};

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

    let params = CreateRunParams::new(run_id, fake_flow_id.clone(), vec![ValueRef::new(json!({}))]);
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
                result: stepflow_core::FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
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
                result: stepflow_core::FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
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

    // Verify that checkpoints were actually created during execution.
    // The InMemoryStateStore tracks total put_checkpoint calls.
    let put_count = store.checkpoint_put_count();
    assert!(
        put_count > 0,
        "Expected at least one checkpoint to be created during execution, got 0"
    );

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
    let params = CreateRunParams::new(root_run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
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
                result: stepflow_core::FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
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

#[tokio::test]
async fn test_recovery_skips_completed_subflow_runstate() {
    // Test that recovery succeeds when the journal contains a completed subflow
    // (RunCompleted event). The completed subflow's RunState should not be
    // reconstructed — its results are in the metadata store.
    use stepflow_core::workflow::FlowBuilder;

    let env = create_test_env().await;
    let orchestrator_id = OrchestratorId::new("test-orch");
    let metadata_store = env.metadata_store();
    let blob_store = env.blob_store();
    let journal = env.execution_journal();

    // Create a 1-step flow
    let flow = Arc::new(
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
            .build(),
    );

    let flow_id = blob_store
        .store_flow(flow)
        .await
        .expect("should store flow");

    let root_run_id = uuid::Uuid::now_v7();
    let completed_subflow_id = uuid::Uuid::now_v7();
    let subflow_key = uuid::Uuid::now_v7();

    // Create root run record
    let params = CreateRunParams::new(root_run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
    metadata_store
        .create_run(params)
        .await
        .expect("should create run");

    // Also create the completed subflow's run record + results in metadata store
    let mut subflow_params = CreateRunParams::new_subflow(
        completed_subflow_id,
        flow_id.clone(),
        vec![ValueRef::new(json!({}))],
        root_run_id,
        root_run_id,
    );
    subflow_params.workflow_name = Some("subflow".to_string());
    metadata_store
        .create_run(subflow_params)
        .await
        .expect("should create subflow run");
    metadata_store
        .record_item_result(
            completed_subflow_id,
            0,
            stepflow_core::FlowResult::Success(ValueRef::new(json!({"sub": "done"}))),
            Vec::new(),
        )
        .await
        .expect("should record subflow result");
    metadata_store
        .update_run_status(completed_subflow_id, ExecutionStatus::Completed)
        .await
        .expect("should update subflow status");

    // Write root events
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
        .unwrap();
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
        .unwrap();

    // Write completed subflow events (including RunCompleted)
    journal
        .write(
            root_run_id,
            JournalEvent::SubflowSubmitted {
                parent_run_id: root_run_id,
                item_index: 0,
                step_index: 0,
                subflow_key,
                subflow_run_id: completed_subflow_id,
            },
        )
        .await
        .unwrap();
    journal
        .write(
            root_run_id,
            JournalEvent::RunCreated {
                run_id: completed_subflow_id,
                flow_id: flow_id.clone(),
                inputs: vec![ValueRef::new(json!({}))],
                variables: HashMap::new(),
                parent_run_id: Some(root_run_id),
            },
        )
        .await
        .unwrap();
    journal
        .write(
            root_run_id,
            JournalEvent::RunCompleted {
                run_id: completed_subflow_id,
                status: ExecutionStatus::Completed,
            },
        )
        .await
        .unwrap();

    // Crash here — root step0 never ran. Recovery should skip the completed
    // subflow's RunState (no flow blob needed for it) and resume the root.

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

    // Root should have completed
    let run = metadata_store
        .get_run(root_run_id)
        .await
        .expect("should get run")
        .expect("run should exist");
    assert_eq!(
        run.summary.status,
        ExecutionStatus::Completed,
        "Root run should complete after recovery with completed subflow in journal"
    );
}
