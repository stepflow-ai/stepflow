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

//! Debug executor for step-by-step workflow execution.
//!
//! [`DebugExecutor`] provides a queue/eval API for interactive debugging of workflows.
//! It composes a [`FlowExecutor`] with task addition control, allowing steps to be
//! queued and executed incrementally rather than all at once.
//!
//! # Architecture
//!
//! DebugExecutor is a thin wrapper around FlowExecutor that provides:
//! - **Task addition control**: `queue_step()` adds steps to the needed set
//! - **Delegation to FlowExecutor**: `run_queue()` calls `FlowExecutor::run()`
//! - **Debug queue persistence**: For session recovery across HTTP requests
//!
//! The key insight is that debug mode differs from batch mode only in *when*
//! steps are added to the needed set, not in how they are executed.

use std::collections::HashMap;
use std::sync::Arc;

use bit_set::BitSet;
use error_stack::ResultExt as _;
use stepflow_core::BlobId;
use stepflow_core::status::{StepExecution, StepStatus};
use stepflow_core::workflow::Flow;
use stepflow_core::{FlowResult, values::ValueRef};
use stepflow_state::StateStore;
use uuid::Uuid;

use crate::flow_executor::{FlowExecutor, FlowExecutorBuilder};
use crate::scheduler::DepthFirstScheduler;
use crate::state::StepIndex;
use crate::step_runner::{StepMetadata, StepRunResult};
use crate::{ExecutionError, Result, StepflowExecutor};

/// Detailed inspection information for a step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepInspection {
    #[serde(flatten)]
    pub metadata: StepMetadata,
    pub input: stepflow_core::ValueExpr,
    pub on_error: stepflow_core::workflow::ErrorAction,
    pub state: StepStatus,
}

/// Debug executor for step-by-step workflow execution.
///
/// Provides the queue/eval API for interactive debugging:
/// - `queue_step()` / `queue_must_execute()` - add steps to execution queue
/// - `run_next_step()` / `run_queue()` - execute queued steps
/// - `eval_step()` / `eval_output()` - run to completion
/// - `get_queued_steps()` / `get_step_result()` - inspect state
///
/// Internally composes a [`FlowExecutor`] with a single item (item_index=0).
pub struct DebugExecutor {
    /// Underlying flow executor (single-item).
    flow_executor: FlowExecutor,
    /// State store for debug queue persistence.
    state_store: Arc<dyn StateStore>,
    /// Run ID for this execution.
    run_id: Uuid,
    /// Flow reference for step lookups.
    flow: Arc<Flow>,
    /// Step ID to index mapping.
    step_index_map: Arc<StepIndex>,
}

impl DebugExecutor {
    /// Create a new debug executor for the given workflow and input.
    ///
    /// This is for fresh executions. Use [`resume()`](Self::resume) to resume
    /// an existing execution from the state store.
    pub async fn new(
        executor: Arc<StepflowExecutor>,
        flow: Arc<Flow>,
        flow_id: BlobId,
        run_id: Uuid,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
        variables: Option<HashMap<String, ValueRef>>,
    ) -> Result<Self> {
        Self::new_internal(
            executor,
            flow,
            flow_id,
            run_id,
            input,
            state_store,
            variables,
            false,
        )
        .await
    }

    /// Internal constructor with validation control.
    #[allow(clippy::too_many_arguments)]
    async fn new_internal(
        executor: Arc<StepflowExecutor>,
        flow: Arc<Flow>,
        flow_id: BlobId,
        run_id: Uuid,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
        variables: Option<HashMap<String, ValueRef>>,
        skip_validation: bool,
    ) -> Result<Self> {
        // Build step index map
        let step_index_map = Arc::new(StepIndex::from_flow(&flow));

        // Create FlowExecutor with single item (no initialization - debug adds steps incrementally)
        // Validation happens in FlowExecutorBuilder::build()
        let mut builder =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .run_id(run_id)
                .input(input)
                .variables(variables.unwrap_or_default())
                .scheduler(Box::new(DepthFirstScheduler::new()));

        if skip_validation {
            builder = builder.skip_validation();
        }

        let flow_executor = builder.build().await?;

        Ok(Self {
            flow_executor,
            state_store,
            run_id,
            flow,
            step_index_map,
        })
    }

    /// Resume an existing execution from the state store.
    ///
    /// This loads the workflow and input from storage, creates a new executor,
    /// and recovers the execution state (completed steps, debug queue).
    pub async fn resume(executor: Arc<StepflowExecutor>, run_id: Uuid) -> Result<Self> {
        let state_store = executor.state_store();

        // Get the run record
        let execution = state_store
            .get_run(run_id)
            .await
            .change_context(ExecutionError::StateError)?
            .ok_or_else(|| error_stack::report!(ExecutionError::ExecutionNotFound(run_id)))?;

        // Get the flow from blob storage
        let flow_id = execution.summary.flow_id;
        let flow = state_store
            .get_flow(&flow_id)
            .await
            .change_context(ExecutionError::StateError)?
            .ok_or_else(|| {
                error_stack::report!(ExecutionError::WorkflowNotFound(flow_id.clone()))
            })?;

        // Get input (first item for now, variables not yet supported)
        let input = execution.inputs.first().cloned().unwrap_or_default();

        // Create the executor (create_run is idempotent, so this is safe)
        // Skip validation since the flow was already validated when first created
        let mut debug_executor = Self::new_internal(
            executor,
            flow,
            flow_id,
            run_id,
            input,
            state_store,
            None, // Variables not supported in debug sessions yet
            true, // skip_validation
        )
        .await?;

        // Recover state from the state store
        let corrections_made = debug_executor.restore_from_state_store().await?;
        if corrections_made > 0 {
            log::info!(
                "Recovery completed for run {}: fixed {} status mismatches",
                run_id,
                corrections_made
            );
        }

        Ok(debug_executor)
    }

    /// Restore state from the state store for session recovery.
    ///
    /// This loads completed step results and the debug queue, then reconciles
    /// the in-memory state with persisted state.
    ///
    /// Returns the number of status corrections made during recovery.
    async fn restore_from_state_store(&mut self) -> Result<usize> {
        // Step 1: Get all completed step results from the state store
        let step_results = self
            .state_store
            .list_step_results(self.run_id)
            .await
            .change_context(ExecutionError::StateError)?;

        // Step 2: Mark completed steps in the item state
        let mut recovered_steps = BitSet::new();
        for step_result in &step_results {
            let step_index = step_result.step_idx();

            // Mark the step as completed in FlowExecutor's state
            // This properly tracks the incomplete counter
            self.flow_executor
                .recover_step(0, step_index, step_result.result().clone());
            recovered_steps.insert(step_index);

            log::debug!(
                "Recovered step {} ({})",
                step_index,
                self.flow.step(step_index).id
            );
        }

        // Step 3: Get current step statuses from the state store
        let step_info_list = self
            .state_store
            .get_step_info_for_execution(self.run_id)
            .await
            .change_context(ExecutionError::StateError)?;

        let mut current_statuses = HashMap::new();
        for step_info in step_info_list {
            current_statuses.insert(step_info.step_index, step_info.status);
        }

        // Step 4: Identify steps that should be runnable but aren't
        let should_be_runnable = self.flow_executor.state().item(0).ready_steps();
        let mut steps_to_fix = BitSet::new();

        for step_index in should_be_runnable.iter() {
            // Only fix steps that are not already completed
            if !recovered_steps.contains(step_index) {
                let current_status = current_statuses
                    .get(&step_index)
                    .copied()
                    .unwrap_or(StepStatus::Blocked);

                if current_status != StepStatus::Runnable {
                    steps_to_fix.insert(step_index);
                    log::info!(
                        "Recovery: fixing status for step {} ({}) from {:?} to Runnable",
                        step_index,
                        self.flow.step(step_index).id,
                        current_status
                    );
                }
            }
        }

        // Step 5: Update status for steps that need fixing
        let corrections_made = steps_to_fix.len();
        if corrections_made > 0 {
            self.state_store
                .queue_write(stepflow_state::StateWriteOperation::UpdateStepStatuses {
                    run_id: self.run_id,
                    status: StepStatus::Runnable,
                    step_indices: steps_to_fix,
                })
                .change_context(ExecutionError::StateError)?;

            log::info!(
                "Recovery completed: recovered {} completed steps, fixed {} status mismatches",
                recovered_steps.len(),
                corrections_made
            );
        } else {
            log::debug!(
                "Recovery completed: recovered {} completed steps, no status corrections needed",
                recovered_steps.len()
            );
        }

        // Step 6: Recover the debug queue if present
        if let Some(queued_step_ids) = self
            .state_store
            .get_debug_queue(self.run_id)
            .await
            .change_context(ExecutionError::StateError)?
        {
            for step_id in &queued_step_ids {
                // Find step index and add to needed (will re-discover deps if needed)
                if let Some(step_index) = self.step_index_map.step_index(step_id) {
                    // Only add if not already completed
                    if !self.flow_executor.state().item(0).is_completed(step_index) {
                        self.flow_executor.add_needed(0, step_index);
                    }
                }
            }
            log::debug!(
                "Recovered debug queue with {} steps: [{}]",
                queued_step_ids.len(),
                queued_step_ids.join(", ")
            );
        }

        Ok(corrections_made)
    }

    /// Get the execution ID for this executor.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Get a reference to the flow being executed.
    pub fn flow(&self) -> &Arc<Flow> {
        &self.flow
    }

    // ========================================================================
    // Private Helpers
    // ========================================================================

    /// Get read access to the single item's state.
    fn item_state(&self) -> &crate::state::ItemState {
        self.flow_executor.state().item(0)
    }

    /// Get the ready step indices.
    fn ready_steps(&self) -> BitSet {
        self.item_state().ready_steps()
    }

    /// Get the result of a completed step by index.
    fn step_result(&self, step_index: usize) -> Option<FlowResult> {
        self.item_state().get_step_result(step_index).cloned()
    }

    /// Check if a step is completed.
    fn is_step_completed(&self, step_index: usize) -> bool {
        self.item_state().is_completed(step_index)
    }

    /// Resolve the output expression.
    fn resolve_output(&self) -> FlowResult {
        let item = self.item_state();
        self.flow.output().resolve(item)
    }

    // ========================================================================
    // Queue/Eval API
    // ========================================================================

    /// Queue a step (and its dependencies) for execution.
    ///
    /// This uses lazy evaluation to discover all transitive dependencies.
    /// The newly queued steps are automatically persisted to the debug queue.
    /// Returns the list of newly queued step IDs.
    pub async fn queue_step(&mut self, step_id: &str) -> Result<Vec<String>> {
        let step_index = self.step_index_map.step_index(step_id).ok_or_else(|| {
            ExecutionError::StepNotFound {
                step: step_id.to_string(),
            }
        })?;

        let newly_needed = self.flow_executor.add_needed(0, step_index);

        let newly_queued: Vec<String> = newly_needed
            .iter()
            .map(|idx| self.flow.step(*idx).id.clone())
            .collect();

        // Persist the newly queued steps
        if !newly_queued.is_empty() {
            self.state_store
                .add_to_debug_queue(self.run_id, &newly_queued)
                .await
                .change_context(ExecutionError::StateError)?;
        }

        log::debug!(
            "Queued step '{}'. Newly discovered: [{}]",
            step_id,
            newly_queued.join(", ")
        );

        Ok(newly_queued)
    }

    /// Queue all steps with `must_execute: true`.
    ///
    /// The newly queued steps are automatically persisted to the debug queue.
    /// Returns the list of queued step IDs (including their dependencies).
    pub async fn queue_must_execute(&mut self) -> Result<Vec<String>> {
        let must_execute_indices: Vec<usize> = self
            .flow
            .steps()
            .iter()
            .enumerate()
            .filter(|(_, step)| step.must_execute())
            .map(|(idx, _)| idx)
            .collect();

        let mut all_newly_queued = Vec::new();
        for idx in must_execute_indices {
            let newly_needed = self.flow_executor.add_needed(0, idx);
            for dep_idx in newly_needed {
                all_newly_queued.push(self.flow.step(dep_idx).id.clone());
            }
        }

        // Persist the newly queued steps
        if !all_newly_queued.is_empty() {
            self.state_store
                .add_to_debug_queue(self.run_id, &all_newly_queued)
                .await
                .change_context(ExecutionError::StateError)?;
        }

        log::debug!(
            "Queued must_execute steps. Newly discovered: [{}]",
            all_newly_queued.join(", ")
        );

        Ok(all_newly_queued)
    }

    /// Evaluate a step: queue it, run all dependencies, return the result.
    ///
    /// This is idempotent - if the step is already completed, returns the cached result.
    pub async fn eval_step(&mut self, step_id: &str) -> Result<FlowResult> {
        let step_index = self.step_index_map.step_index(step_id).ok_or_else(|| {
            ExecutionError::StepNotFound {
                step: step_id.to_string(),
            }
        })?;

        // If already completed, return cached result
        if let Some(result) = self.step_result(step_index) {
            return Ok(result);
        }

        // Queue the step (discovers dependencies)
        self.flow_executor.add_needed(0, step_index);

        // Run until target step completes
        while !self.is_step_completed(step_index) {
            let ready = self.ready_steps();
            if ready.is_empty() {
                return Err(error_stack::report!(ExecutionError::Deadlock)
                    .attach_printable(format!("Cannot make progress towards step '{}'", step_id)));
            }

            // Execute all ready steps
            self.flow_executor.run(None).await?;
        }

        Ok(self.step_result(step_index).unwrap())
    }

    /// Evaluate the workflow output: queue dependencies, run them, return resolved output.
    ///
    /// This also ensures all `must_execute` steps complete successfully.
    pub async fn eval_output(&mut self) -> Result<FlowResult> {
        // Queue output dependencies
        let output_needs = self.flow.output().needed_steps(self.item_state());
        for idx in output_needs.iter() {
            self.flow_executor.add_needed(0, idx);
        }

        // Queue must_execute steps
        for (idx, step) in self.flow.steps().iter().enumerate() {
            if step.must_execute() {
                self.flow_executor.add_needed(0, idx);
            }
        }

        // Run until complete
        loop {
            // Check if all needed steps are complete
            let ready = self.ready_steps();
            if ready.is_empty() {
                // No more ready steps - check if we're done or deadlocked
                if self.item_state().is_complete() {
                    break;
                }
                return Err(error_stack::report!(ExecutionError::Deadlock)
                    .attach_printable("Cannot make progress towards output"));
            }

            // Execute ready steps
            self.flow_executor.run(None).await?;
        }

        // Check for must_execute step failures
        for (step_index, step) in self.flow.steps().iter().enumerate() {
            if step.must_execute()
                && let Some(FlowResult::Failed(error)) = self.step_result(step_index)
            {
                return Ok(FlowResult::Failed(error));
            }
        }

        Ok(self.resolve_output())
    }

    /// Run the next ready step from the queue.
    ///
    /// The executed step is automatically removed from the persistent debug queue.
    /// Returns the execution result, or None if no steps are ready.
    pub async fn run_next_step(&mut self) -> Result<Option<StepRunResult>> {
        let result = self.flow_executor.run_single_task().await?;

        // Remove executed step from persistent queue
        if let Some(ref step_result) = result {
            self.state_store
                .remove_from_debug_queue(
                    self.run_id,
                    std::slice::from_ref(&step_result.metadata.step_id),
                )
                .await
                .change_context(ExecutionError::StateError)?;
        }

        Ok(result)
    }

    /// Run all steps in the queue until empty.
    ///
    /// The executed steps are automatically removed from the persistent debug queue.
    /// Returns all execution results.
    pub async fn run_queue(&mut self) -> Result<Vec<StepRunResult>> {
        let mut results = Vec::new();

        loop {
            let ready_before = self.ready_steps();
            if ready_before.is_empty() {
                break;
            }

            // Capture which steps are ready before running
            let ready_indices: Vec<usize> = ready_before.iter().collect();

            // Run until no more ready steps
            self.flow_executor.run(None).await?;

            // Collect results for the steps that were ready
            for step_index in ready_indices {
                if let Some(result) = self.step_result(step_index) {
                    let step = self.flow.step(step_index);
                    results.push(StepRunResult::new(
                        step_index,
                        step.id.clone(),
                        step.component.to_string(),
                        result,
                    ));
                }
            }
        }

        // Remove all executed steps from persistent queue
        if !results.is_empty() {
            let step_ids: Vec<String> =
                results.iter().map(|r| r.metadata.step_id.clone()).collect();
            self.state_store
                .remove_from_debug_queue(self.run_id, &step_ids)
                .await
                .change_context(ExecutionError::StateError)?;
        }

        Ok(results)
    }

    /// Get steps that are queued (needed) but not yet completed.
    pub fn get_queued_steps(&self) -> Vec<StepExecution> {
        let mut queued = Vec::new();
        let ready = self.ready_steps();
        let item = self.item_state();

        for (idx, step) in self.flow.steps().iter().enumerate() {
            if item.is_needed(idx) && !item.is_completed(idx) {
                let status = if ready.contains(idx) {
                    StepStatus::Runnable
                } else {
                    StepStatus::Blocked
                };
                queued.push(StepExecution::new(
                    idx,
                    step.id.clone(),
                    step.component.to_string(),
                    status,
                ));
            }
        }

        queued
    }

    /// Get the cached result of a step, or None if not yet completed.
    pub fn get_step_result(&self, step_id: &str) -> Option<FlowResult> {
        let step_index = self.step_index_map.step_index(step_id)?;
        self.step_result(step_index)
    }

    // ========================================================================
    // Query Methods
    // ========================================================================

    /// Get access to the state store for querying step results.
    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        &self.state_store
    }

    /// List all steps in the workflow with their current status.
    pub async fn list_all_steps(&self) -> Vec<StepExecution> {
        // Get step statuses from persistent storage
        let status_map = self
            .state_store
            .get_step_info_for_execution(self.run_id)
            .await
            .ok()
            .map(|infos| {
                infos
                    .into_iter()
                    .map(|info| (info.step_index, info.status))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        // Build step execution list
        let ready = self.ready_steps();
        let mut step_statuses = Vec::new();

        for (idx, step) in self.flow.steps().iter().enumerate() {
            let state = status_map.get(&idx).copied().unwrap_or_else(|| {
                if ready.contains(idx) {
                    StepStatus::Runnable
                } else {
                    StepStatus::Blocked
                }
            });

            step_statuses.push(StepExecution::new(
                idx,
                step.id.clone(),
                step.component.to_string(),
                state,
            ));
        }

        step_statuses
    }

    /// Get all completed steps with their results.
    pub async fn get_completed_steps(&self) -> Result<Vec<StepRunResult>> {
        let step_results = self
            .state_store
            .list_step_results(self.run_id)
            .await
            .change_context(ExecutionError::StateError)?;

        let completed_steps = step_results
            .into_iter()
            .map(|step_result| {
                let step_index = step_result.step_idx();
                StepRunResult::new(
                    step_index,
                    step_result.step_id().to_string(),
                    self.flow.step(step_index).component.to_string(),
                    step_result.into_result(),
                )
            })
            .collect();

        Ok(completed_steps)
    }

    /// Execute the workflow to completion using parallel execution with lazy evaluation.
    ///
    /// This is equivalent to: `queue_must_execute() + eval_output()`
    pub async fn execute_to_completion(&mut self) -> Result<FlowResult> {
        // Record start time for metrics
        let start_time = std::time::Instant::now();

        log::debug!("Starting execution of {} steps", self.flow.steps().len());

        // Queue must_execute steps
        self.queue_must_execute().await?;

        // Evaluate output (runs all dependencies)
        let result = self.eval_output().await?;

        // Record metrics
        let duration = start_time.elapsed().as_secs_f64();
        let outcome = match &result {
            FlowResult::Success { .. } => "success",
            FlowResult::Failed { .. } => "failed",
        };
        stepflow_observability::record_workflow_execution(outcome, duration);

        Ok(result)
    }

    /// Get the output/result of a specific step by ID.
    pub fn get_step_output(&self, step_id: &str) -> Result<FlowResult> {
        let step_index = self.step_index_map.step_index(step_id).ok_or_else(|| {
            ExecutionError::StepNotFound {
                step: step_id.to_string(),
            }
        })?;

        self.step_result(step_index).ok_or_else(|| {
            ExecutionError::StepNotCompleted {
                step: step_id.to_string(),
            }
            .into()
        })
    }

    /// Get the details of a specific step for inspection.
    pub async fn inspect_step(&self, step_id: &str) -> Result<StepInspection> {
        let step_index = self.step_index_map.step_index(step_id).ok_or_else(|| {
            ExecutionError::StepNotFound {
                step: step_id.to_string(),
            }
        })?;

        let step = self.flow.step(step_index);
        let ready = self.ready_steps();

        let state = if ready.contains(step_index) {
            StepStatus::Runnable
        } else {
            // Check if step is completed by querying state
            match self.step_result(step_index) {
                Some(FlowResult::Success(_)) => StepStatus::Completed,
                Some(FlowResult::Failed { .. }) => StepStatus::Failed,
                None => StepStatus::Blocked,
            }
        };

        Ok(StepInspection {
            metadata: StepMetadata {
                step_index,
                step_id: step.id.clone(),
                component: step.component.to_string(),
            },
            input: step.input.clone(),
            on_error: step.on_error_or_default(),
            state,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{DebugExecutorBuilder, create_two_step_chain_flow};
    use serde_json::json;
    use stepflow_core::ValueExpr;
    use stepflow_core::workflow::{FlowBuilder, StepBuilder};
    use stepflow_mock::{MockComponentBehavior, MockPlugin};
    use stepflow_state::InMemoryStateStore;

    #[tokio::test]
    async fn test_queue_step_discovers_dependencies() {
        let mut debug = DebugExecutorBuilder::new()
            .with_two_step_chain()
            .build()
            .await;

        // Queue step2 - should discover step1 as dependency
        let queued = debug.queue_step("step2").await.unwrap();
        assert!(queued.contains(&"step1".to_string()));
        assert!(queued.contains(&"step2".to_string()));

        // step1 should be ready (no deps), step2 blocked
        let ready = debug.ready_steps();
        assert!(ready.contains(0)); // step1
        assert!(!ready.contains(1)); // step2 waiting on step1
    }

    #[tokio::test]
    async fn test_run_next_step() {
        let mut debug = DebugExecutorBuilder::new().with_single_step().build().await;

        // Queue step1
        debug.queue_step("step1").await.unwrap();

        // Run next step
        let result = debug.run_next_step().await.unwrap();
        assert!(result.is_some());

        // No more steps ready
        let result = debug.run_next_step().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_step_result() {
        let mut debug = DebugExecutorBuilder::new().with_single_step().build().await;

        // Before execution - no result
        assert!(debug.get_step_result("step1").is_none());

        // Execute step1
        debug.queue_step("step1").await.unwrap();
        debug.run_next_step().await.unwrap();

        // After execution - has result
        let result = debug.get_step_result("step1");
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), FlowResult::Success(_)));
    }

    #[tokio::test]
    async fn test_eval_step() {
        let mut debug = DebugExecutorBuilder::new()
            .with_two_step_chain()
            .build()
            .await;

        // Eval step2 - should automatically run step1 first
        let result = debug.eval_step("step2").await.unwrap();
        assert!(matches!(result, FlowResult::Success(_)));

        // Both steps should be completed
        assert!(debug.get_step_result("step1").is_some());
        assert!(debug.get_step_result("step2").is_some());
    }

    #[tokio::test]
    async fn test_resume_recovers_completed_steps() {
        // Test that resume() properly recovers completed step results from the state store
        let state_store = Arc::new(InMemoryStateStore::new());
        let flow = Arc::new(create_two_step_chain_flow());
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let run_id = Uuid::now_v7();

        // Store the flow as a blob
        state_store
            .put_blob(
                ValueRef::new(serde_json::to_value(flow.as_ref()).unwrap()),
                stepflow_core::BlobType::Flow,
            )
            .await
            .unwrap();

        // Create the run record with input
        let input = ValueRef::new(json!({"value": 1}));
        let mut run_params =
            stepflow_state::CreateRunParams::new(run_id, flow_id.clone(), vec![input.clone()]);
        run_params.workflow_name = flow.name().map(|s: &str| s.to_string());
        state_store.create_run(run_params).await.unwrap();

        // Record step1 as completed
        let step1_result = FlowResult::Success(ValueRef::new(json!({"step1": "done"})));
        state_store
            .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                run_id,
                step_result: stepflow_state::StepResult::new(0, "step1", step1_result.clone()),
            })
            .unwrap();
        // Flush pending writes
        state_store.flush_pending_writes(run_id).await.unwrap();

        // Create executor with same state store
        let mut mock_plugin = MockPlugin::new();
        let behavior = MockComponentBehavior::result(FlowResult::Success(ValueRef::new(
            json!({"result": "ok"}),
        )));
        for input in &[json!({"value": 1}), json!({"step1": "done"})] {
            mock_plugin
                .mock_component("/mock/test")
                .behavior(ValueRef::new(input.clone()), behavior.clone());
        }
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
        let executor = StepflowExecutor::new(
            state_store.clone(),
            std::path::PathBuf::from("."),
            plugin_router,
        );

        // Resume the execution
        let mut debug = DebugExecutor::resume(executor, run_id).await.unwrap();

        // step1 should already be completed (recovered from state store)
        let step1_recovered = debug.get_step_result("step1");
        assert!(
            step1_recovered.is_some(),
            "step1 should be recovered from state store"
        );
        assert!(matches!(step1_recovered.unwrap(), FlowResult::Success(_)));

        // step2 should NOT be completed yet
        assert!(debug.get_step_result("step2").is_none());

        // Queue step2 and run it
        debug.queue_step("step2").await.unwrap();
        let ready = debug.ready_steps();
        assert!(
            ready.contains(1),
            "step2 should be ready since step1 is already completed"
        );

        // Run step2
        let result = debug.run_next_step().await.unwrap();
        assert!(result.is_some());
        assert!(debug.get_step_result("step2").is_some());
    }

    #[tokio::test]
    async fn test_resume_recovers_debug_queue() {
        // Test that resume() recovers the debug queue from the state store
        let state_store = Arc::new(InMemoryStateStore::new());
        let flow = Arc::new(create_two_step_chain_flow());
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let run_id = Uuid::now_v7();

        // Store the flow as a blob
        state_store
            .put_blob(
                ValueRef::new(serde_json::to_value(flow.as_ref()).unwrap()),
                stepflow_core::BlobType::Flow,
            )
            .await
            .unwrap();

        // Create the run record
        let input = ValueRef::new(json!({"value": 1}));
        let mut run_params =
            stepflow_state::CreateRunParams::new(run_id, flow_id.clone(), vec![input.clone()]);
        run_params.workflow_name = flow.name().map(|s: &str| s.to_string());
        state_store.create_run(run_params).await.unwrap();

        // Add steps to the debug queue
        state_store
            .add_to_debug_queue(run_id, &["step1".to_string(), "step2".to_string()])
            .await
            .unwrap();

        // Create executor
        let mut mock_plugin = MockPlugin::new();
        let behavior = MockComponentBehavior::result(FlowResult::Success(ValueRef::new(
            json!({"result": "ok"}),
        )));
        for input in &[json!({"value": 1}), json!({"result": "ok"})] {
            mock_plugin
                .mock_component("/mock/test")
                .behavior(ValueRef::new(input.clone()), behavior.clone());
        }
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
        let executor = StepflowExecutor::new(
            state_store.clone(),
            std::path::PathBuf::from("."),
            plugin_router,
        );

        // Resume the execution
        let debug = DebugExecutor::resume(executor, run_id).await.unwrap();

        // The queued steps should be needed (queue was recovered)
        let queued = debug.get_queued_steps();
        assert!(!queued.is_empty(), "Debug queue should be recovered");

        // step1 should be runnable (recovered from queue)
        let ready = debug.ready_steps();
        assert!(
            ready.contains(0),
            "step1 should be runnable after queue recovery"
        );
    }

    #[tokio::test]
    async fn test_resume_not_found() {
        // Test that resume() fails gracefully when run doesn't exist
        let state_store = Arc::new(InMemoryStateStore::new());

        let mut mock_plugin = MockPlugin::new();
        mock_plugin.mock_component("/mock/test");
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
        let executor =
            StepflowExecutor::new(state_store, std::path::PathBuf::from("."), plugin_router);

        // Try to resume a non-existent run
        let result = DebugExecutor::resume(executor, Uuid::now_v7()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_eval_output() {
        let mut debug = DebugExecutorBuilder::new()
            .with_two_step_chain()
            .build()
            .await;

        // eval_output should queue output dependencies and run them
        let result = debug.eval_output().await.unwrap();
        assert!(matches!(result, FlowResult::Success(_)));

        // Both steps should be completed
        assert!(debug.get_step_result("step1").is_some());
        assert!(debug.get_step_result("step2").is_some());
    }

    #[tokio::test]
    async fn test_run_queue() {
        let mut debug = DebugExecutorBuilder::new()
            .with_two_step_chain()
            .build()
            .await;

        // Queue step2 (which discovers step1 as dependency)
        debug.queue_step("step2").await.unwrap();

        // Run the queue - should execute all ready steps until queue is empty
        let results = debug.run_queue().await.unwrap();

        // Both steps should be executed (run_queue runs until no more ready)
        // Results may be 1 or 2 depending on scheduler batching behavior
        assert!(!results.is_empty());
        assert!(debug.get_step_result("step1").is_some());
        assert!(debug.get_step_result("step2").is_some());
    }

    fn create_must_execute_flow() -> Arc<Flow> {
        Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("optional")
                        .component("/mock/test")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                    StepBuilder::new("required")
                        .component("/mock/test")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .must_execute(true)
                        .build(),
                ])
                .output(ValueExpr::Input {
                    input: Default::default(),
                })
                .build(),
        )
    }

    #[tokio::test]
    async fn test_queue_must_execute() {
        let mut debug = DebugExecutorBuilder::new()
            .with_flow(create_must_execute_flow())
            .build()
            .await;

        // Queue must_execute steps
        let queued = debug.queue_must_execute().await.unwrap();

        // Should queue the "required" step
        assert!(queued.contains(&"required".to_string()));
        // Should NOT queue the "optional" step
        assert!(!queued.contains(&"optional".to_string()));
    }

    #[tokio::test]
    async fn test_execute_to_completion() {
        let mut debug = DebugExecutorBuilder::new()
            .with_two_step_chain()
            .build()
            .await;

        // Execute to completion
        let result = debug.execute_to_completion().await.unwrap();
        assert!(matches!(result, FlowResult::Success(_)));

        // All steps should be completed
        assert!(debug.get_step_result("step1").is_some());
        assert!(debug.get_step_result("step2").is_some());
    }

    #[tokio::test]
    async fn test_queue_persistence() {
        let (mut debug, state_store) = DebugExecutorBuilder::new()
            .with_two_step_chain()
            .build_with_state_store()
            .await;

        let run_id = debug.run_id();

        // Queue a step
        debug.queue_step("step1").await.unwrap();

        // Check state store has the queue
        let persisted_queue = state_store.get_debug_queue(run_id).await.unwrap();
        assert!(persisted_queue.is_some());
        assert!(persisted_queue.unwrap().contains(&"step1".to_string()));
    }

    #[tokio::test]
    async fn test_run_removes_from_queue() {
        let (mut debug, state_store) = DebugExecutorBuilder::new()
            .with_single_step()
            .build_with_state_store()
            .await;

        let run_id = debug.run_id();

        // Queue and run step1
        debug.queue_step("step1").await.unwrap();

        // Verify it's in the queue
        let queue_before = state_store.get_debug_queue(run_id).await.unwrap();
        assert!(queue_before.is_some());
        assert!(queue_before.unwrap().contains(&"step1".to_string()));

        // Run the step
        debug.run_next_step().await.unwrap();

        // Check queue is now empty
        let queue_after = state_store.get_debug_queue(run_id).await.unwrap();
        assert!(
            queue_after.is_none() || queue_after.unwrap().is_empty(),
            "Queue should be empty after running the step"
        );
    }

    #[tokio::test]
    async fn test_get_queued_steps() {
        let mut debug = DebugExecutorBuilder::new()
            .with_two_step_chain()
            .build()
            .await;

        // Initially no queued steps
        assert!(debug.get_queued_steps().is_empty());

        // Queue step2 (which discovers step1 as dependency)
        debug.queue_step("step2").await.unwrap();

        // Should have 2 queued steps
        let queued = debug.get_queued_steps();
        assert_eq!(queued.len(), 2);

        // step1 should be Runnable, step2 should be Blocked
        let step1_status = queued.iter().find(|s| s.step_id == "step1").unwrap();
        let step2_status = queued.iter().find(|s| s.step_id == "step2").unwrap();
        assert_eq!(step1_status.status, StepStatus::Runnable);
        assert_eq!(step2_status.status, StepStatus::Blocked);
    }

    #[tokio::test]
    async fn test_list_all_steps() {
        let debug = DebugExecutorBuilder::new()
            .with_two_step_chain()
            .build()
            .await;

        // List all steps
        let all_steps = debug.list_all_steps().await;
        assert_eq!(all_steps.len(), 2);

        // Should have step1 and step2
        let step_ids: Vec<&str> = all_steps.iter().map(|s| s.step_id.as_str()).collect();
        assert!(step_ids.contains(&"step1"));
        assert!(step_ids.contains(&"step2"));
    }

    #[tokio::test]
    async fn test_get_step_output() {
        let mut debug = DebugExecutorBuilder::new().with_single_step().build().await;

        // Before execution - should error
        let result = debug.get_step_output("step1");
        assert!(result.is_err());

        // Execute step1
        debug.queue_step("step1").await.unwrap();
        debug.run_next_step().await.unwrap();

        // After execution - should succeed
        let result = debug.get_step_output("step1");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), FlowResult::Success(_)));

        // Non-existent step should error
        let result = debug.get_step_output("nonexistent");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_inspect_step() {
        let debug = DebugExecutorBuilder::new().with_single_step().build().await;

        // Inspect step1
        let inspection = debug.inspect_step("step1").await.unwrap();
        assert_eq!(inspection.metadata.step_id, "step1");
        assert_eq!(inspection.metadata.component, "/mock/test");
        assert_eq!(inspection.state, StepStatus::Blocked); // Not queued yet

        // Non-existent step should error
        let result = debug.inspect_step("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_eval_step_idempotent() {
        let mut debug = DebugExecutorBuilder::new().with_single_step().build().await;

        // First eval
        let result1 = debug.eval_step("step1").await.unwrap();
        assert!(matches!(result1, FlowResult::Success(_)));

        // Second eval - should return cached result
        let result2 = debug.eval_step("step1").await.unwrap();
        assert!(matches!(result2, FlowResult::Success(_)));
    }
}
