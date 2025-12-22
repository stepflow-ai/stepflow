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

use std::{collections::HashMap, sync::Arc};

use bit_set::BitSet;
use error_stack::ResultExt as _;
use stepflow_core::BlobId;
use stepflow_core::status::{StepExecution, StepStatus};
use stepflow_core::{
    FlowResult,
    values::{StepContext as _, ValueRef},
    workflow::{Flow, WorkflowOverrides},
};
use stepflow_observability::{RunInfoGuard, StepIdGuard};
use stepflow_plugin::{DynPlugin, ExecutionContext, Plugin as _};
use stepflow_state::{StateStore, StepResult};
use uuid::Uuid;

use crate::{ExecutionError, Result, StepflowExecutor};

/// Execute a workflow and return the result.
pub(crate) async fn execute_workflow(
    executor: Arc<StepflowExecutor>,
    flow: Arc<Flow>,
    flow_id: BlobId,
    run_id: Uuid,
    input: ValueRef,
    state_store: Arc<dyn StateStore>,
    variables: Option<HashMap<String, ValueRef>>,
) -> Result<FlowResult> {
    // Set run_id in diagnostic context for all logs in this workflow execution
    let _run_guard = RunInfoGuard::new(flow_id.to_string(), run_id.to_string());

    log::info!(
        "Starting workflow execution: flow={}, run_id={}",
        flow.name().unwrap_or("unnamed"),
        run_id
    );

    // Store workflow first (this is idempotent if workflow already exists)
    let computed_hash = state_store
        .store_flow(flow.clone())
        .await
        .change_context(ExecutionError::StateError)?;

    // Verify the hash matches (should be the same if workflow is deterministic)
    if computed_hash != flow_id {
        log::warn!("Flow hash mismatch: expected {flow_id}, computed {computed_hash}");
    }

    // Create run record in state store before starting workflow
    state_store
        .create_run(stepflow_state::CreateRunParams {
            run_id,
            flow_id: flow_id.clone(),
            workflow_name: flow.name().map(|s| s.to_string()),
            workflow_label: None, // No label for direct execution
            debug_mode: false,    // Not debug mode
            input: input.clone(),
            overrides: WorkflowOverrides::default(), // No overrides for direct execution
            variables: variables.clone().unwrap_or_default(), // Variables for execution
        })
        .await
        .change_context(ExecutionError::StateError)?;

    let mut workflow_executor = WorkflowExecutor::new(
        executor,
        flow,
        flow_id,
        run_id,
        input,
        state_store,
        variables,
    )?;

    let result = workflow_executor.execute_to_completion().await;

    match &result {
        Ok(FlowResult::Success(_)) => {
            log::info!("Workflow execution completed successfully");
        }
        Ok(FlowResult::Failed(error)) => {
            log::warn!("Workflow execution failed: {}", error.message);
        }
        Err(e) => {
            log::error!("Workflow execution error: {e:?}");
        }
    }

    result
}

/// Workflow executor that manages the execution of a single workflow.
///
/// This serves as the core execution engine that can be used directly for
/// run-to-completion execution, or controlled step-by-step by the debug session.
pub struct WorkflowExecutor {
    /// Execution tracker for managing workflow execution state
    tracker: crate::tracker::ExecutionTracker,
    /// State store for step results
    state_store: Arc<dyn StateStore>,
    /// Executor for getting plugins and execution context
    executor: Arc<StepflowExecutor>,
    /// The workflow being executed
    flow: Arc<Flow>,
    /// Execution context for this session
    context: ExecutionContext,
}

impl WorkflowExecutor {
    /// Create a new workflow executor for the given workflow and input.
    pub fn new(
        executor: Arc<StepflowExecutor>,
        flow: Arc<Flow>,
        flow_id: BlobId,
        run_id: Uuid,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
        variables: Option<HashMap<String, ValueRef>>,
    ) -> Result<Self> {
        // Validate the workflow before execution.
        // This is a sanity check since the Flow has been validated before executing.
        let diagnostics =
            stepflow_analysis::validate(&flow).change_context(ExecutionError::AnalysisError)?;

        if diagnostics.has_fatal() {
            let fatal = diagnostics.num_fatal;
            let error = diagnostics.num_error;
            return Err(
                error_stack::report!(ExecutionError::AnalysisError).attach_printable(format!(
                    "Workflow validation failed with {fatal} fatal and {error} error diagnostics"
                )),
            );
        }

        // Create tracker with workflow input and variables
        let tracker = crate::tracker::ExecutionTracker::new(
            &flow,
            input.clone(),
            variables.clone().unwrap_or_default(),
        );

        // Create workflow-aware execution context
        let context = ExecutionContext::for_workflow_with_flow(
            executor.clone(),
            run_id,
            flow.clone(),
            flow_id,
        );

        Ok(Self {
            tracker,
            state_store,
            executor,
            flow,
            context,
        })
    }

    /// Get the execution ID for this executor.
    pub fn run_id(&self) -> Uuid {
        self.context.run_id()
    }

    /// Get a reference to the flow being executed.
    pub fn flow(&self) -> &Arc<Flow> {
        &self.flow
    }

    /// Get currently runnable step indices.
    pub fn get_runnable_step_indices(&self) -> BitSet {
        self.tracker.ready_steps()
    }

    /// Recover execution state from the state store and fix any missed status transitions.
    ///
    /// This method should be called when resuming a workflow execution (e.g., debug sessions,
    /// workflow restart) to ensure the dependency tracker accurately reflects completed steps
    /// and that step statuses are correctly updated.
    ///
    /// Recovery process:
    /// 1. Query all completed step results from the state store
    /// 2. Mark completed steps in the dependency tracker
    /// 3. Identify steps that should be runnable based on completed dependencies
    /// 4. Update status for steps marked as blocked but should be runnable
    ///
    /// # Returns
    /// The number of step status corrections made during recovery
    pub async fn recover_from_state_store(&mut self) -> Result<usize> {
        let run_id = self.context.run_id();

        // Step 1: Get all completed step results from the state store
        let step_results = self
            .state_store
            .list_step_results(run_id)
            .await
            .change_context(ExecutionError::StateError)?;

        // Step 2: Mark completed steps in the dependency tracker
        let mut recovered_steps = BitSet::new();
        for step_result in &step_results {
            let step_index = step_result.step_idx();

            // Mark the step as completed in the tracker
            let newly_unblocked = self
                .tracker
                .complete_step(step_index, step_result.result().clone());
            recovered_steps.insert(step_index);

            log::debug!(
                "Recovered step {} ({}), newly unblocked: [{}]",
                step_index,
                self.flow.step(step_index).id,
                newly_unblocked
                    .iter()
                    .map(|idx| self.flow.step(idx).id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        // Step 3: Get current step statuses from the state store
        let step_info_list = self
            .state_store
            .get_step_info_for_execution(run_id)
            .await
            .change_context(ExecutionError::StateError)?;

        let mut current_statuses = std::collections::HashMap::new();
        for step_info in step_info_list {
            current_statuses.insert(step_info.step_index, step_info.status);
        }

        // Step 4: Identify steps that should be runnable but aren't
        let should_be_runnable = self.tracker.ready_steps();
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
            // Update in state store
            self.state_store
                .queue_write(stepflow_state::StateWriteOperation::UpdateStepStatuses {
                    run_id,
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
            .get_debug_queue(run_id)
            .await
            .change_context(ExecutionError::StateError)?
        {
            for step_id in &queued_step_ids {
                // Find step index and add to needed (will re-discover deps if needed)
                if let Some(step_index) = self
                    .flow
                    .steps()
                    .iter()
                    .position(|step| &step.id == step_id)
                {
                    // Only add if not already completed
                    if !self.tracker.is_completed(step_index) {
                        self.tracker.add_or_update_needed(step_index, &self.flow);
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

    /// Execute the workflow to completion using parallel execution with lazy evaluation.
    ///
    /// This is equivalent to: `queue_must_execute() + eval_output()`
    pub async fn execute_to_completion(&mut self) -> Result<FlowResult> {
        // Record start time for metrics
        let start_time = std::time::Instant::now();

        log::debug!("Starting execution of {} steps", self.flow.steps().len());

        // Queue must_execute steps
        self.queue_must_execute();

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

    /// List all steps in the workflow with their current status.
    pub async fn list_all_steps(&self) -> Vec<StepExecution> {
        // Get step info from persistent storage (single query for all steps)
        let step_infos = self
            .state_store
            .get_step_info_for_execution(self.context.run_id())
            .await
            .unwrap_or_default();

        // NOTE: We could assume that the steps come back in order, and then
        // treat gaps as `None`. This would allow us to avoid hashing.

        // Create a map from step_index to status for efficient lookup
        let mut status_map = std::collections::HashMap::new();
        for step_info in step_infos {
            status_map.insert(step_info.step_index, step_info.status);
        }

        // Build step execution list using cached workflow data + persistent status
        let mut step_statuses = Vec::new();
        for (idx, step) in self.flow.steps().iter().enumerate() {
            let state = status_map.get(&idx).copied().unwrap_or_else(|| {
                // Fallback: check if step is runnable using in-memory tracker
                let runnable = self.tracker.ready_steps();
                if runnable.contains(idx) {
                    StepStatus::Runnable
                } else {
                    // Default to blocked for steps without persistent status
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

    /// Get currently runnable steps.
    pub async fn get_runnable_steps(&self) -> Vec<StepExecution> {
        // Query state store for runnable steps (based on status only)
        let runnable_step_infos = self
            .state_store
            .get_runnable_steps(self.context.run_id())
            .await
            .unwrap_or_default();

        // Convert step infos to step executions using cached workflow data
        runnable_step_infos
            .iter()
            .map(|step_info| {
                let step = &self.flow.step(step_info.step_index);
                StepExecution::new(
                    step_info.step_index,
                    step.id.clone(),
                    step.component.to_string(),
                    StepStatus::Runnable,
                )
            })
            .collect()
    }

    /// Execute specific steps by their IDs.
    /// Returns execution results for each step.
    /// Only executes steps that are currently runnable.`
    pub async fn execute_steps(&mut self, step_ids: &[String]) -> Result<Vec<StepExecutionResult>> {
        let mut results = Vec::new();

        for step_id in step_ids {
            results.push(self.execute_step_by_id(step_id).await?);
        }

        Ok(results)
    }

    /// Execute all currently runnable steps.
    /// Returns execution results for each step.
    pub async fn execute_all_runnable(&mut self) -> Result<Vec<StepExecutionResult>> {
        let runnable_steps = self.get_runnable_steps().await;
        let step_ids: Vec<String> = runnable_steps.iter().map(|s| s.step_id.clone()).collect();
        self.execute_steps(&step_ids).await
    }

    /// Execute a specific step by ID.
    /// Returns an error if the step is not currently runnable.
    pub async fn execute_step_by_id(&mut self, step_id: &str) -> Result<StepExecutionResult> {
        // Find the step by ID
        let step_index = self
            .flow
            .steps()
            .iter()
            .position(|step| step.id == step_id)
            .ok_or_else(|| ExecutionError::StepNotFound {
                step: step_id.to_string(),
            })?;

        self.execute_step_by_index(step_index).await
    }

    /// Continue execution until a specific step is runnable.
    /// Returns results of all steps that were executed during this operation.
    pub async fn execute_until_runnable(
        &mut self,
        target_step_id: &str,
    ) -> Result<Vec<StepExecutionResult>> {
        let mut executed_steps = Vec::new();

        // Find the target step index
        let target_step_index = self
            .flow
            .steps()
            .iter()
            .position(|step| step.id == target_step_id)
            .ok_or_else(|| ExecutionError::StepNotFound {
                step: target_step_id.to_string(),
            })?;

        // Keep executing until the target step is runnable or completed
        loop {
            let runnable = self.tracker.ready_steps();

            // Check if target step is runnable
            if runnable.contains(target_step_index) {
                break;
            }

            // If no steps are runnable, we can't make progress
            if runnable.is_empty() {
                log::warn!("No runnable steps. Unable to progress.");
                break;
            }

            // Execute all currently runnable steps
            for step_index in runnable.iter() {
                executed_steps.push(self.execute_step_by_index(step_index).await?);
            }
        }

        Ok(executed_steps)
    }

    /// Get all completed steps with their results.
    pub async fn get_completed_steps(&self) -> Result<Vec<StepExecutionResult>> {
        let step_results = self
            .state_store
            .list_step_results(self.context.run_id())
            .await
            .change_context(ExecutionError::StateError)?;

        let completed_steps = step_results
            .into_iter()
            .map(|step_result| StepExecutionResult {
                metadata: StepMetadata {
                    step_index: step_result.step_idx(),
                    step_id: step_result.step_id().to_string(),
                    component: self.flow.step(step_result.step_idx()).component.to_string(),
                },
                result: step_result.into_result(),
            })
            .collect();

        Ok(completed_steps)
    }

    /// Get the output/result of a specific step by ID.
    pub fn get_step_output(&self, step_id: &str) -> Result<FlowResult> {
        let step_index =
            self.tracker
                .step_index(step_id)
                .ok_or_else(|| ExecutionError::StepNotFound {
                    step: step_id.to_string(),
                })?;

        self.tracker.get_result(step_index).cloned().ok_or_else(|| {
            ExecutionError::StepNotCompleted {
                step: step_id.to_string(),
            }
            .into()
        })
    }

    // ========================================================================
    // Queue/Eval API - Unified primitives for normal execution and debugging
    // ========================================================================

    /// Queue a step (and its dependencies) for execution.
    ///
    /// This uses lazy evaluation to discover all transitive dependencies.
    /// Returns the list of newly queued step IDs.
    pub fn queue_step(&mut self, step_id: &str) -> Result<Vec<String>> {
        let step_index = self
            .flow
            .steps()
            .iter()
            .position(|step| step.id == step_id)
            .ok_or_else(|| ExecutionError::StepNotFound {
                step: step_id.to_string(),
            })?;

        let newly_needed = self.tracker.add_or_update_needed(step_index, &self.flow);

        let newly_queued: Vec<String> = newly_needed
            .iter()
            .map(|idx| self.flow.step(idx).id.clone())
            .collect();

        log::debug!(
            "Queued step '{}'. Newly discovered: [{}]",
            step_id,
            newly_queued.join(", ")
        );

        Ok(newly_queued)
    }

    /// Queue all steps with `must_execute: true`.
    ///
    /// Returns the list of queued step IDs (including their dependencies).
    pub fn queue_must_execute(&mut self) -> Vec<String> {
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
            let newly_needed = self.tracker.add_or_update_needed(idx, &self.flow);
            for dep_idx in newly_needed.iter() {
                all_newly_queued.push(self.flow.step(dep_idx).id.clone());
            }
        }

        log::debug!(
            "Queued must_execute steps. Newly discovered: [{}]",
            all_newly_queued.join(", ")
        );

        all_newly_queued
    }

    /// Evaluate a step: queue it, run all dependencies, return the result.
    ///
    /// This is idempotent - if the step is already completed, returns the cached result.
    pub async fn eval_step(&mut self, step_id: &str) -> Result<FlowResult> {
        let step_index = self
            .flow
            .steps()
            .iter()
            .position(|step| step.id == step_id)
            .ok_or_else(|| ExecutionError::StepNotFound {
                step: step_id.to_string(),
            })?;

        // If already completed, return cached result
        if let Some(result) = self.tracker.get_result(step_index) {
            return Ok(result.clone());
        }

        // Queue the step (discovers dependencies)
        self.tracker.add_or_update_needed(step_index, &self.flow);

        // Run until target step completes
        while !self.tracker.is_completed(step_index) {
            let ready = self.tracker.ready_steps();
            if ready.is_empty() {
                return Err(error_stack::report!(ExecutionError::Deadlock)
                    .attach_printable(format!("Cannot make progress towards step '{}'", step_id)));
            }

            // Execute all ready steps
            for idx in ready.iter() {
                self.execute_step_by_index(idx).await?;
                // Re-evaluate dependencies after each completion
                self.tracker.add_or_update_needed(step_index, &self.flow);
            }
        }

        Ok(self.tracker.get_result(step_index).unwrap().clone())
    }

    /// Evaluate the workflow output: queue dependencies, run them, return resolved output.
    ///
    /// This also ensures all `must_execute` steps complete successfully.
    pub async fn eval_output(&mut self) -> Result<FlowResult> {
        // Queue output dependencies
        let output_needs = self.flow.output().needed_steps(&self.tracker);
        for idx in output_needs.iter() {
            self.tracker.add_or_update_needed(idx, &self.flow);
        }

        // Run until output is resolvable and all needed steps complete
        loop {
            // Check if output is ready
            let output_needs = self.flow.output().needed_steps(&self.tracker);
            if output_needs.is_empty() && self.tracker.all_needed_completed() {
                break;
            }

            // Queue any newly discovered output dependencies
            for idx in output_needs.iter() {
                self.tracker.add_or_update_needed(idx, &self.flow);
            }

            let ready = self.tracker.ready_steps();
            if ready.is_empty() {
                if !self.tracker.all_needed_completed() {
                    return Err(error_stack::report!(ExecutionError::Deadlock)
                        .attach_printable("Cannot make progress towards output"));
                }
                break;
            }

            // Execute all ready steps (execute_step_by_index handles completion)
            for idx in ready.iter() {
                self.execute_step_by_index(idx).await?;
            }
        }

        // Check for must_execute step failures
        for (step_index, step) in self.flow.steps().iter().enumerate() {
            if step.must_execute()
                && let Some(FlowResult::Failed(error)) = self.tracker.get_result(step_index)
            {
                return Ok(FlowResult::Failed(error.clone()));
            }
        }

        Ok(self.resolve_workflow_output())
    }

    /// Run the next ready step from the queue.
    ///
    /// Returns the execution result, or None if no steps are ready.
    pub async fn run_next_step(&mut self) -> Result<Option<StepExecutionResult>> {
        let ready = self.tracker.ready_steps();
        if ready.is_empty() {
            return Ok(None);
        }

        // Get the first ready step
        let step_index = ready.iter().next().unwrap();
        let result = self.execute_step_by_index(step_index).await?;

        Ok(Some(result))
    }

    /// Run all steps in the queue until empty.
    ///
    /// Returns all execution results.
    pub async fn run_queue(&mut self) -> Result<Vec<StepExecutionResult>> {
        let mut results = Vec::new();

        loop {
            let ready = self.tracker.ready_steps();
            if ready.is_empty() {
                break;
            }

            for idx in ready.iter() {
                let result = self.execute_step_by_index(idx).await?;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Get steps that are queued (needed) but not yet completed.
    pub fn get_queued_steps(&self) -> Vec<StepExecution> {
        let mut queued = Vec::new();
        let ready = self.tracker.ready_steps();

        for (idx, step) in self.flow.steps().iter().enumerate() {
            if self.tracker.is_needed(idx) && !self.tracker.is_completed(idx) {
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
        let step_index = self.tracker.step_index(step_id)?;
        self.tracker.get_result(step_index).cloned()
    }

    /// Add steps to the persistent debug queue.
    ///
    /// This should be called after `queue_step()` returns newly queued steps.
    /// The queue persists across HTTP requests for step-through debugging.
    pub async fn add_steps_to_debug_queue(&self, step_ids: &[String]) -> Result<()> {
        if step_ids.is_empty() {
            return Ok(());
        }

        self.state_store
            .add_to_debug_queue(self.context.run_id(), step_ids)
            .await
            .change_context(ExecutionError::StateError)?;

        log::debug!(
            "Added to debug queue for run {}: [{}]",
            self.context.run_id(),
            step_ids.join(", ")
        );

        Ok(())
    }

    /// Remove steps from the persistent debug queue.
    ///
    /// This should be called after executing steps via `run_next_step()` or `run_queue()`.
    pub async fn remove_steps_from_debug_queue(&self, step_ids: &[String]) -> Result<()> {
        if step_ids.is_empty() {
            return Ok(());
        }

        self.state_store
            .remove_from_debug_queue(self.context.run_id(), step_ids)
            .await
            .change_context(ExecutionError::StateError)?;

        log::debug!(
            "Removed from debug queue for run {}: [{}]",
            self.context.run_id(),
            step_ids.join(", ")
        );

        Ok(())
    }

    // ========================================================================
    // End Queue/Eval API
    // ========================================================================

    /// Get the details of a specific step for inspection.
    pub async fn inspect_step(&self, step_id: &str) -> Result<StepInspection> {
        let step_index = self
            .flow
            .steps()
            .iter()
            .position(|step| step.id == step_id)
            .ok_or_else(|| ExecutionError::StepNotFound {
                step: step_id.to_string(),
            })?;

        let step = &self.flow.step(step_index);
        let runnable = self.tracker.ready_steps();

        let state = if runnable.contains(step_index) {
            StepStatus::Runnable
        } else {
            // Check if step is completed by querying state store
            match self
                .state_store
                .get_step_result(self.context.run_id(), step_index)
                .await
            {
                Ok(result) => match result {
                    FlowResult::Success(_) => StepStatus::Completed,
                    FlowResult::Failed { .. } => StepStatus::Failed,
                },
                Err(_) => StepStatus::Blocked,
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

    /// Execute a single step by index and record the result.
    pub async fn execute_step_by_index(
        &mut self,
        step_index: usize,
    ) -> Result<StepExecutionResult> {
        let step = &self.flow.step(step_index);
        let step_id = step.id.clone();
        let component_string = step.component.to_string();

        // Check if the step is runnable
        if !self.tracker.ready_steps().contains(step_index) {
            return Err(ExecutionError::StepNotRunnable {
                step: step.id.clone(),
            }
            .into());
        }

        // Resolve step inputs
        let step_input = match step.input.resolve(&self.tracker) {
            FlowResult::Success(result) => result,
            FlowResult::Failed(error) => {
                // Record the failure as a step completion to prevent infinite loops
                let result = FlowResult::Failed(error);
                self.record_step_completion(step_index, &result).await?;
                return Ok(StepExecutionResult::new(
                    step_index,
                    step_id,
                    component_string,
                    result,
                ));
            }
        };

        // Get plugin and resolved component name
        let (plugin, resolved_component) = self
            .executor
            .get_plugin_and_component(&step.component, step_input.clone())
            .await?;
        // Create step-specific execution context reusing the workflow context
        let step_context = self.context.with_step(step_id.clone());

        let result =
            execute_step_async(plugin, step, &resolved_component, step_input, step_context).await?;

        // Record the result
        self.record_step_completion(step_index, &result).await?;

        Ok(StepExecutionResult::new(
            step_index,
            step_id,
            component_string,
            result,
        ))
    }

    /// Record step completion and update tracker.
    pub async fn record_step_completion(
        &mut self,
        step_index: usize,
        result: &FlowResult,
    ) -> Result<()> {
        // Update tracker (stores result internally)
        self.tracker.complete_step(step_index, result.clone());

        // Record in state store (non-blocking)
        let step_id = &self.flow.step(step_index).id;
        self.state_store
            .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                run_id: self.context.run_id(),
                step_result: StepResult::new(step_index, step_id, result.clone()),
            })
            .change_context(ExecutionError::StateError)?;

        Ok(())
    }

    /// Resolve the workflow output.
    pub fn resolve_workflow_output(&self) -> FlowResult {
        self.flow.output().resolve(&self.tracker)
    }

    /// Get access to the state store for querying step results.
    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        &self.state_store
    }
}

/// Execute a single step asynchronously.
pub(crate) async fn execute_step_async(
    plugin: &Arc<DynPlugin<'static>>,
    step: &stepflow_core::workflow::Step,
    resolved_component: &str,
    input: ValueRef,
    context: ExecutionContext,
) -> Result<FlowResult> {
    use stepflow_observability::fastrace::prelude::*;

    // Set step_id in diagnostic context for all logs in this step execution
    let _step_guard = StepIdGuard::new(step.id.as_str());

    // Create a span for this step execution
    let span = Span::enter_with_local_parent("step")
        .with_property(|| ("step_id", step.id.clone()))
        .with_property(|| ("component", resolved_component.to_string()));

    let result = async move {
        log::debug!(
            "Executing step: component={}, step_id={}",
            resolved_component,
            step.id
        );

        // Create a component from the resolved component name
        let component = stepflow_core::workflow::Component::from_string(resolved_component);

        // Execute the component
        let result = plugin
            .execute(&component, context, input)
            .await
            .map_err(|error| {
                error
                    .change_context(ExecutionError::StepFailed {
                        step: step.id.to_owned(),
                    })
                    .attach_printable(format!("Component execution failed for step '{}'", step.id))
                    .attach_printable(format!("Component: {resolved_component}"))
            })?;

        log::debug!("Step execution completed: step_id={}", step.id);

        Ok::<_, error_stack::Report<ExecutionError>>(result)
    }
    .in_span(span)
    .await?;

    match &result {
        FlowResult::Failed(error) => {
            match step.on_error_or_default() {
                stepflow_core::workflow::ErrorAction::UseDefault { default_value } => {
                    log::debug!(
                        "Step {} failed but using default value: {:?}",
                        step.id,
                        error
                    );
                    let value = default_value.clone().unwrap_or(serde_json::Value::Null);
                    Ok(FlowResult::Success(ValueRef::new(value)))
                }
                stepflow_core::workflow::ErrorAction::Fail => Ok(result),
                stepflow_core::workflow::ErrorAction::Retry => {
                    // TODO: Implement retry logic
                    log::warn!(
                        "Retry error action not yet implemented for step {}",
                        step.id
                    );
                    Ok(result)
                }
            }
        }
        _ => Ok(result),
    }
}

/// Basic metadata about a step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepMetadata {
    pub step_index: usize,
    pub step_id: String,
    pub component: String,
}

/// Result of executing a step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepExecutionResult {
    #[serde(flatten)]
    pub metadata: StepMetadata,
    pub result: FlowResult,
}

impl StepExecutionResult {
    pub fn new(step_index: usize, step_id: String, component: String, result: FlowResult) -> Self {
        Self {
            metadata: StepMetadata {
                step_index,
                step_id,
                component,
            },
            result,
        }
    }
}

/// Detailed inspection information for a step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepInspection {
    #[serde(flatten)]
    pub metadata: StepMetadata,
    pub input: stepflow_core::ValueExpr,
    pub on_error: stepflow_core::workflow::ErrorAction,
    pub state: StepStatus,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use serde_json::json;
    use stepflow_core::FlowError;
    use stepflow_core::ValueExpr;
    use stepflow_core::workflow::{Flow, FlowBuilder, Step, StepBuilder};
    use stepflow_mock::{MockComponentBehavior, MockPlugin};
    use stepflow_state::InMemoryStateStore;

    /// Helper function to create workflow from YAML string with simple mock behaviors
    /// Each component gets a single behavior that will be used regardless of input
    pub async fn create_workflow_from_yaml_simple(
        yaml_str: &str,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> (Arc<crate::executor::StepflowExecutor>, Arc<Flow>, BlobId) {
        // Parse the YAML workflow
        let flow: Flow = serde_yaml_ng::from_str(yaml_str).expect("Failed to parse YAML workflow");
        let flow = Arc::new(flow);

        // Create executor with mock plugin
        let mut mock_plugin = MockPlugin::new();

        // Configure mock behaviors - use a wildcard approach where any input gets the same behavior
        for (component_name, result) in mock_behaviors {
            let behavior = MockComponentBehavior::result(result.clone());

            // Add behavior for multiple common input patterns that might occur
            let common_inputs = vec![
                json!({}),
                json!({"message": "hello"}),
                json!({"value": 10}),
                json!({"value": 42}),
                json!({"result": 20}),
                json!({"step1": {"step1": "done"}, "step2": {"step2": "done"}}),
                json!({"mode": "error"}),
                json!({"mode": "succeed"}),
            ];

            for input in common_inputs {
                mock_plugin
                    .mock_component(component_name)
                    .behavior(ValueRef::new(input), behavior.clone());
            }
        }

        let dyn_plugin = stepflow_plugin::DynPlugin::boxed(mock_plugin);

        // Create a plugin router with mock plugin routing rules
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

        let executor = crate::executor::StepflowExecutor::new(
            Arc::new(InMemoryStateStore::new()),
            std::path::PathBuf::from("."),
            plugin_router,
        );

        // Generate flow hash for testing
        let flow_id = BlobId::from_flow(flow.as_ref()).unwrap();
        (executor, flow, flow_id)
    }

    /// Execute a workflow from YAML string with given input
    pub async fn execute_workflow_from_yaml_simple(
        yaml_str: &str,
        input: serde_json::Value,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> Result<FlowResult> {
        let (executor, flow, flow_id) =
            create_workflow_from_yaml_simple(yaml_str, mock_behaviors).await;
        let run_id = Uuid::now_v7();
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let input_ref = ValueRef::new(input);

        execute_workflow(
            executor,
            flow,
            flow_id,
            run_id,
            input_ref,
            state_store,
            None,
        )
        .await
    }

    /// Create a WorkflowExecutor from YAML string for step-by-step testing.
    /// This initializes the queue with must_execute and output dependencies.
    pub async fn create_workflow_executor_from_yaml_simple(
        yaml_str: &str,
        input: serde_json::Value,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> Result<WorkflowExecutor> {
        let (executor, flow, flow_id) =
            create_workflow_from_yaml_simple(yaml_str, mock_behaviors).await;
        let run_id = Uuid::now_v7();
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let input_ref = ValueRef::new(input);

        let mut workflow_executor = WorkflowExecutor::new(
            executor,
            flow,
            flow_id,
            run_id,
            input_ref,
            state_store,
            None,
        )?;
        // Initialize the queue for normal execution (queue output deps + must_execute)
        workflow_executor.queue_must_execute();
        // Also queue output dependencies for proper test setup
        let output_needs = workflow_executor
            .flow
            .output()
            .needed_steps(&workflow_executor.tracker);
        for idx in output_needs.iter() {
            workflow_executor
                .tracker
                .add_or_update_needed(idx, &workflow_executor.flow);
        }
        Ok(workflow_executor)
    }

    fn create_test_step(id: &str, input: serde_json::Value) -> Step {
        StepBuilder::mock_step(id).input_json(input).build()
    }

    fn create_test_flow(steps: Vec<Step>, output: ValueExpr) -> Flow {
        FlowBuilder::new().steps(steps).output(output).build()
    }

    #[tokio::test]
    async fn test_dependency_tracking_basic() {
        // Test that we can create dependencies and tracker correctly
        let steps = vec![
            create_test_step("step1", json!({"value": 42})),
            create_test_step("step2", json!({"$step": "step1"})),
        ];

        let flow = Arc::new(create_test_flow(
            steps,
            ValueExpr::Step {
                step: "step2".to_string(),
                path: Default::default(),
            },
        ));

        // Create tracker - starts empty with dynamic evaluation
        let mut tracker = crate::tracker::ExecutionTracker::new(
            &flow,
            stepflow_core::values::ValueRef::new(serde_json::Value::Null),
            std::collections::HashMap::new(),
        );

        // Initially empty - no steps needed yet
        assert_eq!(tracker.ready_steps().len(), 0);

        // Evaluate what the output needs using needed_steps()
        // The output is { $step: step2 }, which needs step2
        let output_needs = flow.output().needed_steps(&tracker);
        assert!(output_needs.contains(1)); // step2 is needed

        // Add output deps directly (dynamic evaluation, no static transitive deps)
        for idx in output_needs.iter() {
            tracker.add_needed(idx);
        }

        // Evaluate step2's needs - it depends on step1
        let step2_needs = flow.step(1).input.needed_steps(&tracker);
        assert!(step2_needs.contains(0)); // step2 needs step1

        // Set up waiting_on for step2 and add step1 to needed
        tracker.set_waiting(1, step2_needs.clone());
        for idx in step2_needs.iter() {
            tracker.add_needed(idx);
        }

        // Only step1 should be ready (step2 is waiting on step1)
        let ready = tracker.ready_steps();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(0)); // step1 is ready

        // Complete step1 - step2 should become unblocked
        let newly_unblocked = tracker.complete_step(
            0,
            FlowResult::Success(ValueRef::new(json!({"result": "done"}))),
        );
        assert!(newly_unblocked.contains(1)); // step2 is now unblocked

        // Now step2 should be ready
        let ready = tracker.ready_steps();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(1)); // step2 is now ready

        // This confirms the tracker integration with lazy evaluation is working
    }

    #[tokio::test]
    async fn test_simple_workflow_execution() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/simple
    input:
      $input: "$"
output:
  $step: step1
"#;

        let input_value = json!({"message": "hello"});
        let mock_behaviors = vec![(
            "/mock/simple",
            FlowResult::Success(ValueRef::new(json!({"output": "processed"}))),
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, input_value, mock_behaviors)
            .await
            .unwrap();

        match result {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!({"output": "processed"}));
            }
            _ => panic!("Expected successful result, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn test_step_dependencies() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/first
    input:
      $input: "$"
  - id: step2
    component: /mock/second
    input:
      $step: step1
output:
  $step: step2
"#;

        let workflow_input = json!({"value": 10});
        let step1_output = json!({"result": 20});

        let mock_behaviors = vec![
            (
                "/mock/first",
                FlowResult::Success(ValueRef::new(step1_output.clone())),
            ),
            (
                "/mock/second",
                FlowResult::Success(ValueRef::new(json!({"final": 30}))),
            ),
        ];

        let result =
            execute_workflow_from_yaml_simple(workflow_yaml, workflow_input, mock_behaviors)
                .await
                .unwrap();

        match result {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!({"final": 30}));
            }
            _ => panic!("Expected successful result, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn test_workflow_executor_step_by_step() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/first
    input:
      $input: "$"
  - id: step2
    component: /mock/second
    input:
      $step: step1
output:
  $step: step2
"#;

        let workflow_input = json!({"value": 10});
        let mock_behaviors = vec![
            (
                "/mock/first",
                FlowResult::Success(ValueRef::new(json!({"result": 20}))),
            ),
            (
                "/mock/second",
                FlowResult::Success(ValueRef::new(json!({"final": 30}))),
            ),
        ];

        let mut workflow_executor = create_workflow_executor_from_yaml_simple(
            workflow_yaml,
            workflow_input,
            mock_behaviors,
        )
        .await
        .unwrap();

        // Initially, only step1 should be runnable
        let runnable = workflow_executor.get_runnable_step_indices();
        assert_eq!(runnable.len(), 1);
        assert!(runnable.contains(0)); // step1

        // Execute step1
        let result = workflow_executor.execute_step_by_index(0).await.unwrap();
        match result.result {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!({"result": 20}));
            }
            _ => panic!("Expected successful result from step1"),
        }

        // Now step2 should be runnable
        let runnable = workflow_executor.get_runnable_step_indices();
        assert_eq!(runnable.len(), 1);
        assert!(runnable.contains(1)); // step2

        // Execute step2
        let result = workflow_executor.execute_step_by_index(1).await.unwrap();
        match result.result {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!({"final": 30}));
            }
            _ => panic!("Expected successful result from step2"),
        }

        // No more steps should be runnable
        let runnable = workflow_executor.get_runnable_step_indices();
        assert_eq!(runnable.len(), 0);

        // Resolve final output
        let final_result = workflow_executor.resolve_workflow_output();
        match final_result {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!({"final": 30}));
            }
            _ => panic!("Expected successful final result"),
        }
    }

    #[tokio::test]
    async fn test_workflow_recovery_logic() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/identity
    input:
      value: "step1_output"
  - id: step2
    component: /mock/identity
    input:
      value: { $step: step1 }
output:
  final: { $step: step2 }
"#;

        let workflow: Arc<Flow> = Arc::new(serde_yaml_ng::from_str(workflow_yaml).unwrap());
        let flow_id = BlobId::from_flow(&workflow).unwrap();
        let executor = StepflowExecutor::new_in_memory();
        let run_id = Uuid::now_v7();
        let input = ValueRef::new(json!({}));

        // Create a workflow executor
        let mut workflow_executor = WorkflowExecutor::new(
            executor.clone(),
            workflow.clone(),
            flow_id.clone(),
            run_id,
            input.clone(),
            executor.state_store(),
            None,
        )
        .unwrap();
        // Initialize the queue for normal execution
        workflow_executor.queue_must_execute();
        // Also queue output dependencies
        let output_needs = workflow_executor
            .flow
            .output()
            .needed_steps(&workflow_executor.tracker);
        for idx in output_needs.iter() {
            workflow_executor
                .tracker
                .add_or_update_needed(idx, &workflow_executor.flow);
        }

        // Manually execute step1 to simulate partial execution
        let step1_result = FlowResult::Success(ValueRef::new(json!("step1_output")));

        // Record step1 result directly to state store (bypassing normal execution)
        let step_result = StepResult::new(0, "step1", step1_result);
        workflow_executor
            .state_store
            .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                run_id,
                step_result,
            })
            .unwrap();

        // Flush to ensure step1 result is persisted
        workflow_executor
            .state_store
            .flush_pending_writes(run_id)
            .await
            .unwrap();

        // At this point, step2 should be runnable but the dependency tracker doesn't know about step1 completion
        // and step2 status is not updated to "Runnable"

        // Verify initial state: step1 should be runnable in the tracker (step2 is blocked)
        let runnable_before = workflow_executor.get_runnable_step_indices();
        assert_eq!(runnable_before.len(), 1);
        assert!(runnable_before.contains(0)); // Only step1 should be runnable initially

        // Now run recovery logic
        let corrections_made = workflow_executor.recover_from_state_store().await.unwrap();

        // Verify that recovery found step1 was completed and updated the tracker
        let runnable_after = workflow_executor.get_runnable_step_indices();
        assert_eq!(runnable_after.len(), 1);
        assert!(runnable_after.contains(1)); // Now step2 should be runnable
        assert!(!runnable_after.contains(0)); // step1 should no longer be runnable (completed)

        // Recovery should have made status corrections (step2 should be marked as Runnable)
        assert_eq!(corrections_made, 1);

        // Verify that step1 result is stored in tracker
        let cached_result = workflow_executor.tracker.get_result(0);
        assert!(cached_result.is_some());
        match cached_result.unwrap() {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!("step1_output"));
            }
            _ => panic!("Expected successful result"),
        }
    }

    #[tokio::test]
    async fn test_async_write_behavior() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/identity
    input:
      value: "step1_result"
  - id: step2
    component: /mock/identity
    input:
      value: { $step: step1 }
  - id: step3
    component: /mock/identity
    input:
      value: { $step: step2 }
output:
  final: { $step: step3 }
"#;

        let workflow: Arc<Flow> = Arc::new(serde_yaml_ng::from_str(workflow_yaml).unwrap());
        let flow_id = BlobId::from_flow(&workflow).unwrap();
        let executor = StepflowExecutor::new_in_memory();
        let run_id = Uuid::now_v7();
        let input = ValueRef::new(json!({}));

        let workflow_executor = WorkflowExecutor::new(
            executor.clone(),
            workflow.clone(),
            flow_id.clone(),
            run_id,
            input.clone(),
            executor.state_store(),
            None,
        )
        .unwrap();

        // Test that async writes are non-blocking
        let step1_result = FlowResult::Success(ValueRef::new(json!("step1_result")));
        let step_result = StepResult::new(0, "step1", step1_result.clone());

        // Record multiple step results rapidly (should be queued)
        workflow_executor
            .state_store
            .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                run_id,
                step_result: step_result.clone(),
            })
            .unwrap();
        workflow_executor
            .state_store
            .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                run_id,
                step_result: step_result.clone(),
            })
            .unwrap();
        workflow_executor
            .state_store
            .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                run_id,
                step_result: step_result.clone(),
            })
            .unwrap();

        // These should complete without blocking

        // Test that flush ensures all writes are persisted
        workflow_executor
            .state_store
            .flush_pending_writes(run_id)
            .await
            .unwrap();

        // After flush, we should be able to query the result
        let retrieved_result = workflow_executor
            .state_store
            .get_step_result(run_id, 0)
            .await
            .unwrap();

        match retrieved_result {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!("step1_result"));
            }
            _ => panic!("Expected successful result"),
        }
    }

    #[tokio::test]
    async fn test_recovery_with_complex_dependencies() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/identity
    input:
      value: "step1_result"
  - id: step2
    component: /mock/identity
    input:
      value: "step2_result"
  - id: step3
    component: /mock/identity
    input:
      value: { $step: step1 }
  - id: step4
    component: /mock/identity
    input:
      value: { $step: step2 }
  - id: step5
    component: /mock/identity
    input:
      deps: [{ $step: step3 }, { $step: step4 }]
output:
  final: { $step: step5 }
"#;

        let workflow: Arc<Flow> = Arc::new(serde_yaml_ng::from_str(workflow_yaml).unwrap());
        let flow_id = BlobId::from_flow(&workflow).unwrap();
        let executor = StepflowExecutor::new_in_memory();
        let run_id = Uuid::now_v7();
        let input = ValueRef::new(json!({}));

        let mut workflow_executor = WorkflowExecutor::new(
            executor.clone(),
            workflow.clone(),
            flow_id.clone(),
            run_id,
            input.clone(),
            executor.state_store(),
            None,
        )
        .unwrap();
        // Initialize the queue for normal execution
        workflow_executor.queue_must_execute();
        // Also queue output dependencies
        let output_needs = workflow_executor
            .flow
            .output()
            .needed_steps(&workflow_executor.tracker);
        for idx in output_needs.iter() {
            workflow_executor
                .tracker
                .add_or_update_needed(idx, &workflow_executor.flow);
        }

        // Simulate completed step1 and step2
        let step1_result = StepResult::new(
            0,
            "step1",
            FlowResult::Success(ValueRef::new(json!("step1_result"))),
        );
        let step2_result = StepResult::new(
            1,
            "step2",
            FlowResult::Success(ValueRef::new(json!("step2_result"))),
        );

        workflow_executor
            .state_store
            .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                run_id,
                step_result: step1_result,
            })
            .unwrap();
        workflow_executor
            .state_store
            .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                run_id,
                step_result: step2_result,
            })
            .unwrap();
        workflow_executor
            .state_store
            .flush_pending_writes(run_id)
            .await
            .unwrap();

        // Before recovery: step1 and step2 are runnable (their results not yet in tracker)
        let runnable_before = workflow_executor.get_runnable_step_indices();
        assert_eq!(runnable_before.len(), 2);
        assert!(runnable_before.contains(0)); // step1
        assert!(runnable_before.contains(1)); // step2

        // Run recovery
        let corrections_made = workflow_executor.recover_from_state_store().await.unwrap();

        // After recovery: step3 and step4 should be runnable (step1 and step2 completed)
        let runnable_after = workflow_executor.get_runnable_step_indices();
        assert_eq!(runnable_after.len(), 2);
        assert!(runnable_after.contains(2)); // step3 (depends on step1)
        assert!(runnable_after.contains(3)); // step4 (depends on step2)

        // Should have made corrections for step3 and step4
        assert_eq!(corrections_made, 2);
    }

    #[tokio::test]
    async fn test_recovery_with_fresh_execution() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/identity
    input:
      value: "step1_result"
output:
  final: { $step: step1 }
"#;

        let workflow: Arc<Flow> = Arc::new(serde_yaml_ng::from_str(workflow_yaml).unwrap());
        let flow_id = BlobId::from_flow(&workflow).unwrap();
        let executor = StepflowExecutor::new_in_memory();
        let run_id = Uuid::now_v7();
        let input = ValueRef::new(json!({}));

        let mut workflow_executor = WorkflowExecutor::new(
            executor.clone(),
            workflow.clone(),
            flow_id.clone(),
            run_id,
            input.clone(),
            executor.state_store(),
            None,
        )
        .unwrap();
        // Initialize the queue for normal execution
        workflow_executor.queue_must_execute();
        // Also queue output dependencies
        let output_needs = workflow_executor
            .flow
            .output()
            .needed_steps(&workflow_executor.tracker);
        for idx in output_needs.iter() {
            workflow_executor
                .tracker
                .add_or_update_needed(idx, &workflow_executor.flow);
        }

        // Test with a fresh execution - since there are no completed steps and no step info
        // in the database, recovery should mark the initially runnable step as Runnable
        let corrections_made = workflow_executor.recover_from_state_store().await.unwrap();

        // Since step1 is already runnable from queue_must_execute, recovery should make 1 correction
        assert_eq!(corrections_made, 1);

        // step1 should still be runnable
        let runnable = workflow_executor.get_runnable_step_indices();
        assert_eq!(runnable.len(), 1);
        assert!(runnable.contains(0));
    }

    #[tokio::test]
    async fn test_workflow_executor_parallel_vs_sequential() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/parallel1
    input:
      $input: "$"
  - id: step2
    component: /mock/parallel2
    input:
      $input: "$"
  - id: final
    component: /mock/combiner
    input:
      step1:
        $step: step1
      step2:
        $step: step2
output:
  $step: final
"#;

        let workflow_input = json!({"value": 42});
        let step1_output = json!({"step1": "done"});
        let step2_output = json!({"step2": "done"});
        let final_output = json!({"both": "completed"});

        // Test parallel execution
        let parallel_mock_behaviors = vec![
            (
                "/mock/parallel1",
                FlowResult::Success(ValueRef::new(step1_output.clone())),
            ),
            (
                "/mock/parallel2",
                FlowResult::Success(ValueRef::new(step2_output.clone())),
            ),
            (
                "/mock/combiner",
                FlowResult::Success(ValueRef::new(final_output.clone())),
            ),
        ];

        let parallel_result = execute_workflow_from_yaml_simple(
            workflow_yaml,
            workflow_input.clone(),
            parallel_mock_behaviors,
        )
        .await
        .unwrap();

        // Test sequential execution
        let sequential_mock_behaviors = vec![
            (
                "/mock/parallel1",
                FlowResult::Success(ValueRef::new(step1_output.clone())),
            ),
            (
                "/mock/parallel2",
                FlowResult::Success(ValueRef::new(step2_output.clone())),
            ),
            (
                "/mock/combiner",
                FlowResult::Success(ValueRef::new(final_output.clone())),
            ),
        ];

        let mut sequential_executor = create_workflow_executor_from_yaml_simple(
            workflow_yaml,
            workflow_input,
            sequential_mock_behaviors,
        )
        .await
        .unwrap();

        // Execute all steps sequentially until completion
        while !sequential_executor.get_runnable_step_indices().is_empty() {
            let runnable = sequential_executor.get_runnable_step_indices();
            for step_index in runnable.iter() {
                sequential_executor
                    .execute_step_by_index(step_index)
                    .await
                    .unwrap();
            }
        }
        let sequential_result = sequential_executor.resolve_workflow_output();

        // Both should produce the same result
        match (&parallel_result, &sequential_result) {
            (FlowResult::Success(p), FlowResult::Success(s)) => {
                assert_eq!(p.as_ref(), s.as_ref());
                assert_eq!(p.as_ref(), &final_output);
            }
            _ => panic!("Both executions should be successful"),
        }
    }

    #[tokio::test]
    async fn test_error_handling_skip() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: failing_step
    component: /mock/error
    onError:
      action: useDefault
    input:
      mode: error
output:
  $step: failing_step
"#;

        let mock_behaviors = vec![(
            "/mock/error",
            FlowResult::Failed(FlowError::new(500, "Test error")),
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // The workflow should complete with null (skip returns null now)
        match result {
            FlowResult::Success(value) if value.as_ref().is_null() => {
                // Expected - the step failed but was configured to skip, returns null
            }
            _ => panic!("Expected success with null result (skipped), got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn test_error_handling_use_default_with_value() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: failing_step
    component: /mock/error
    onError:
      action: useDefault
      defaultValue: {"fallback": "value"}
    input:
      mode: error
output:
  $step: failing_step
"#;

        let mock_behaviors = vec![(
            "/mock/error",
            FlowResult::Failed(FlowError::new(500, "Test error")),
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        match result {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!({"fallback": "value"}));
            }
            _ => panic!("Expected success with default value, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn test_error_handling_use_default_without_value() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: failing_step
    component: /mock/error
    onError:
      action: useDefault
    input:
      mode: error
output:
  $step: failing_step
"#;

        let mock_behaviors = vec![(
            "/mock/error",
            FlowResult::Failed(FlowError::new(500, "Test error")),
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        match result {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &serde_json::Value::Null);
            }
            _ => panic!("Expected success with null value, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn test_error_handling_fail() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: failing_step
    component: /mock/error
    onError:
      action: fail
    input:
      mode: error
output:
  $step: failing_step
"#;

        let mock_behaviors = vec![(
            "/mock/error",
            FlowResult::Failed(FlowError::new(500, "Test error")),
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        match result {
            FlowResult::Failed(error) => {
                assert_eq!(error.code, 500);
                assert_eq!(error.message, "Test error");
            }
            _ => panic!("Expected failed result, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn test_error_handling_success_case() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: success_step
    component: /mock/success
    onError:
      action: useDefault
    input: {}
output:
  $step: success_step
"#;

        let mock_behaviors = vec![(
            "/mock/success",
            FlowResult::Success(ValueRef::new(json!({"result":"success"}))),
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        match result {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!({"result": "success"}));
            }
            _ => panic!("Expected success result, got: {result:?}"),
        }
    }

    #[tokio::test]
    async fn test_error_handling_skip_with_coalesce() {
        // Test that when a step is skipped (returns null), $coalesce can provide a fallback
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: failing_step
    component: /mock/error
    onError:
      action: useDefault
    input:
      mode: error
output:
  # Use $coalesce to handle null from skipped step
  $coalesce:
    - { $step: failing_step }
    - { fallback: "skipped" }
"#;

        let mock_behaviors = vec![(
            "/mock/error",
            FlowResult::Failed(FlowError::new(500, "Test error")),
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // The output should use the fallback value since the step was skipped (null)
        match result {
            FlowResult::Success(value) => {
                assert_eq!(value.as_ref(), &json!({"fallback": "skipped"}));
            }
            _ => panic!("Expected success with fallback result, got: {result:?}"),
        }
    }

    /// Create a WorkflowExecutor in debug mode (empty queue) for step-by-step testing.
    pub async fn create_debug_workflow_executor(
        yaml_str: &str,
        input: serde_json::Value,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> Result<WorkflowExecutor> {
        let (executor, flow, flow_id) =
            create_workflow_from_yaml_simple(yaml_str, mock_behaviors).await;
        let run_id = Uuid::now_v7();
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let input_ref = ValueRef::new(input);

        // Debug mode: don't initialize the queue - start empty
        WorkflowExecutor::new(
            executor,
            flow,
            flow_id,
            run_id,
            input_ref,
            state_store,
            None,
        )
    }

    #[tokio::test]
    async fn test_debug_queue_step_discovers_dependencies() {
        // Test that queue_step() correctly discovers and queues dependencies
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/first
    input:
      value: 10
  - id: step2
    component: /mock/second
    input:
      $step: step1
  - id: step3
    component: /mock/third
    input:
      $step: step2
output:
  $step: step3
"#;

        let mock_behaviors = vec![
            (
                "/mock/first",
                FlowResult::Success(ValueRef::new(json!({"a": 1}))),
            ),
            (
                "/mock/second",
                FlowResult::Success(ValueRef::new(json!({"b": 2}))),
            ),
            (
                "/mock/third",
                FlowResult::Success(ValueRef::new(json!({"c": 3}))),
            ),
        ];

        let mut executor = create_debug_workflow_executor(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // Queue should be empty initially
        assert!(executor.get_queued_steps().is_empty());

        // Queue step3 - should discover step2 and step1 as dependencies
        let queued = executor.queue_step("step3").unwrap();
        assert_eq!(queued.len(), 3);
        assert!(queued.contains(&"step1".to_string()));
        assert!(queued.contains(&"step2".to_string()));
        assert!(queued.contains(&"step3".to_string()));

        // Verify the queue state
        let queue_state = executor.get_queued_steps();
        assert_eq!(queue_state.len(), 3);

        // Only step1 should be runnable (step2, step3 are blocked)
        let runnable: Vec<_> = queue_state
            .iter()
            .filter(|s| s.status == stepflow_core::status::StepStatus::Runnable)
            .collect();
        assert_eq!(runnable.len(), 1);
        assert_eq!(runnable[0].step_id, "step1");
    }

    #[tokio::test]
    async fn test_debug_run_next_step() {
        // Test that run_next_step() executes one step at a time
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/first
    input:
      value: 10
  - id: step2
    component: /mock/second
    input:
      $step: step1
output:
  $step: step2
"#;

        let mock_behaviors = vec![
            (
                "/mock/first",
                FlowResult::Success(ValueRef::new(json!({"result": 20}))),
            ),
            (
                "/mock/second",
                FlowResult::Success(ValueRef::new(json!({"final": 30}))),
            ),
        ];

        let mut executor = create_debug_workflow_executor(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // Queue step2 (will also queue step1)
        executor.queue_step("step2").unwrap();

        // Run next step - should execute step1
        let result1 = executor.run_next_step().await.unwrap();
        assert!(result1.is_some());
        let result1 = result1.unwrap();
        assert_eq!(result1.metadata.step_id, "step1");
        match result1.result {
            FlowResult::Success(v) => assert_eq!(v.as_ref(), &json!({"result": 20})),
            _ => panic!("Expected success"),
        }

        // Run next step - should execute step2
        let result2 = executor.run_next_step().await.unwrap();
        assert!(result2.is_some());
        let result2 = result2.unwrap();
        assert_eq!(result2.metadata.step_id, "step2");
        match result2.result {
            FlowResult::Success(v) => assert_eq!(v.as_ref(), &json!({"final": 30})),
            _ => panic!("Expected success"),
        }

        // No more steps to run
        let result3 = executor.run_next_step().await.unwrap();
        assert!(result3.is_none());

        // Queue should be empty (all completed)
        assert!(executor.get_queued_steps().is_empty());
    }

    #[tokio::test]
    async fn test_debug_eval_step() {
        // Test that eval_step() automatically runs dependencies and returns result
        // Using identity-like behavior where output matches input format
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/first
    input:
      value: 10
  - id: step2
    component: /mock/second
    input:
      value: 10
  - id: step3
    component: /mock/third
    input:
      value: 10
output:
  $step: step3
"#;

        let mock_behaviors = vec![
            (
                "/mock/first",
                FlowResult::Success(ValueRef::new(json!({"result": "step1"}))),
            ),
            (
                "/mock/second",
                FlowResult::Success(ValueRef::new(json!({"result": "step2"}))),
            ),
            (
                "/mock/third",
                FlowResult::Success(ValueRef::new(json!({"result": "step3"}))),
            ),
        ];

        let mut executor = create_debug_workflow_executor(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // Eval step2 - should automatically run step1 first (step1 is not a dep of step2 in this case)
        // Actually these are independent steps - let me check
        // step2 input: {value: 10} - doesn't depend on step1
        let result = executor.eval_step("step2").await.unwrap();
        match result {
            FlowResult::Success(v) => assert_eq!(v.as_ref(), &json!({"result": "step2"})),
            _ => panic!("Expected success"),
        }

        // Only step2 should be completed since step1 is not a dependency
        assert!(executor.get_step_result("step1").is_none());
        assert!(executor.get_step_result("step2").is_some());
        assert!(executor.get_step_result("step3").is_none());

        // Eval step2 again - should return cached result (idempotent)
        let result_cached = executor.eval_step("step2").await.unwrap();
        match result_cached {
            FlowResult::Success(v) => assert_eq!(v.as_ref(), &json!({"result": "step2"})),
            _ => panic!("Expected cached success"),
        }
    }

    #[tokio::test]
    async fn test_debug_run_queue() {
        // Test that run_queue() runs all queued steps until empty
        // Using independent steps to simplify mock matching
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/first
    input:
      value: 10
  - id: step2
    component: /mock/second
    input:
      value: 10
  - id: step3
    component: /mock/third
    input:
      value: 10
output:
  result:
    a: { $step: step1, path: result }
    b: { $step: step2, path: result }
    c: { $step: step3, path: result }
"#;

        let mock_behaviors = vec![
            (
                "/mock/first",
                FlowResult::Success(ValueRef::new(json!({"result": "step1"}))),
            ),
            (
                "/mock/second",
                FlowResult::Success(ValueRef::new(json!({"result": "step2"}))),
            ),
            (
                "/mock/third",
                FlowResult::Success(ValueRef::new(json!({"result": "step3"}))),
            ),
        ];

        let mut executor = create_debug_workflow_executor(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // Queue all steps by queueing each explicitly
        executor.queue_step("step1").unwrap();
        executor.queue_step("step2").unwrap();
        executor.queue_step("step3").unwrap();
        assert_eq!(executor.get_queued_steps().len(), 3);

        // Run all queued steps
        let results = executor.run_queue().await.unwrap();
        assert_eq!(results.len(), 3);

        // All steps should have executed (order may vary since they're independent)
        let step_ids: Vec<_> = results
            .iter()
            .map(|r| r.metadata.step_id.as_str())
            .collect();
        assert!(step_ids.contains(&"step1"));
        assert!(step_ids.contains(&"step2"));
        assert!(step_ids.contains(&"step3"));

        // Queue should be empty
        assert!(executor.get_queued_steps().is_empty());
    }

    #[tokio::test]
    async fn test_debug_get_step_result() {
        // Test that get_step_result() returns cached results
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/first
    input:
      value: 10
output:
  $step: step1
"#;

        let mock_behaviors = vec![(
            "/mock/first",
            FlowResult::Success(ValueRef::new(json!({"result": 42}))),
        )];

        let mut executor = create_debug_workflow_executor(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // No result before execution
        assert!(executor.get_step_result("step1").is_none());

        // Queue and run
        executor.queue_step("step1").unwrap();
        executor.run_next_step().await.unwrap();

        // Result should be available
        let result = executor.get_step_result("step1");
        assert!(result.is_some());
        match result.unwrap() {
            FlowResult::Success(v) => assert_eq!(v.as_ref(), &json!({"result": 42})),
            _ => panic!("Expected success"),
        }
    }
}
