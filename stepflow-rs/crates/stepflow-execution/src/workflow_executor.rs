// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use std::sync::Arc;

use bit_set::BitSet;
use error_stack::ResultExt as _;
use futures::{StreamExt as _, future::BoxFuture, stream::FuturesUnordered};
use stepflow_core::BlobId;
use stepflow_core::status::{StepExecution, StepStatus};
use stepflow_core::{
    FlowResult,
    values::{ValueRef, ValueResolver, ValueTemplate},
    workflow::{Expr, Flow},
};
use stepflow_plugin::{DynPlugin, ExecutionContext, Plugin as _};
use stepflow_state::{StateStore, StepResult};
use uuid::Uuid;

use crate::{ExecutionError, Result, StateValueLoader, StepFlowExecutor, write_cache::WriteCache};

/// Execute a workflow and return the result.
pub(crate) async fn execute_workflow(
    executor: Arc<StepFlowExecutor>,
    flow: Arc<Flow>,
    flow_id: BlobId,
    run_id: Uuid,
    input: ValueRef,
    state_store: Arc<dyn StateStore>,
) -> Result<FlowResult> {
    // Store workflow first (this is idempotent if workflow already exists)
    let computed_hash = state_store
        .store_flow(flow.clone())
        .await
        .change_context(ExecutionError::StateError)?;

    // Verify the hash matches (should be the same if workflow is deterministic)
    if computed_hash != flow_id {
        tracing::warn!(
            "Flow hash mismatch: expected {}, computed {}",
            flow_id,
            computed_hash
        );
    }

    // Create run record in state store before starting workflow
    state_store
        .create_run(
            run_id,
            flow_id.clone(),
            flow.name(),
            None,  // No label for direct execution
            false, // Not debug mode
            input.clone(),
        )
        .await
        .change_context(ExecutionError::StateError)?;

    let mut workflow_executor =
        WorkflowExecutor::new(executor, flow, flow_id, run_id, input, state_store)?;

    workflow_executor.execute_to_completion().await
}

/// Workflow executor that manages the execution of a single workflow.
///
/// This serves as the core execution engine that can be used directly for
/// run-to-completion execution, or controlled step-by-step by the debug session.
pub struct WorkflowExecutor {
    /// Dependency tracker for determining runnable steps
    tracker: stepflow_analysis::DependencyTracker,
    /// Value resolver for resolving step inputs
    resolver: ValueResolver<StateValueLoader>,
    /// State store for step results
    state_store: Arc<dyn StateStore>,
    /// Executor for getting plugins and execution context
    executor: Arc<StepFlowExecutor>,
    /// The workflow being executed
    flow: Arc<Flow>,
    /// Execution context for this session
    context: ExecutionContext,
    /// Write-through cache for avoiding unnecessary flushes
    write_cache: WriteCache,
}

impl WorkflowExecutor {
    /// Create a new workflow executor for the given workflow and input.
    pub fn new(
        executor: Arc<StepFlowExecutor>,
        flow: Arc<Flow>,
        flow_id: BlobId,
        run_id: Uuid,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
    ) -> Result<Self> {
        // Build dependencies for the workflow using the analysis crate
        let analysis_result = stepflow_analysis::analyze_flow_dependencies(flow.clone(), flow_id)
            .change_context(ExecutionError::AnalysisError)?;

        let analysis = match analysis_result.analysis {
            Some(analysis) => analysis,
            None => {
                // Convert validation failure to execution error
                let (fatal, error, _warning) = analysis_result.diagnostic_counts();
                return Err(
                    error_stack::report!(ExecutionError::AnalysisError).attach_printable(format!(
                        "Workflow validation failed with {fatal} fatal and {error} error diagnostics"
                    )),
                );
            }
        };

        // Create tracker from analysis
        let tracker = analysis.new_dependency_tracker();

        // Create write cache and step ID mapping
        let write_cache = WriteCache::new(flow.steps().len());

        // Create state value loader and value resolver
        let state_loader = StateValueLoader::new(
            input.clone(),
            state_store.clone(),
            write_cache.clone(),
            flow.clone(),
        );
        let resolver = ValueResolver::new(run_id, input, state_loader, flow.clone());

        // Create execution context
        let context = executor.execution_context(run_id);

        Ok(Self {
            tracker,
            resolver,
            state_store,
            executor,
            write_cache,
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
        self.tracker.unblocked_steps()
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
            let newly_unblocked = self.tracker.complete_step(step_index);
            recovered_steps.insert(step_index);

            // Cache the step result to avoid unnecessary database queries
            self.write_cache
                .cache_step_result(step_index, step_result.result().clone())
                .await;

            tracing::debug!(
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
        let should_be_runnable = self.tracker.unblocked_steps();
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
                    tracing::info!(
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
            // Cache the status updates
            self.write_cache
                .cache_step_statuses(StepStatus::Runnable, &steps_to_fix)
                .await;

            // Update in state store
            self.state_store
                .queue_write(stepflow_state::StateWriteOperation::UpdateStepStatuses {
                    run_id,
                    status: StepStatus::Runnable,
                    step_indices: steps_to_fix,
                })
                .change_context(ExecutionError::StateError)?;

            tracing::info!(
                "Recovery completed: recovered {} completed steps, fixed {} status mismatches",
                recovered_steps.len(),
                corrections_made
            );
        } else {
            tracing::debug!(
                "Recovery completed: recovered {} completed steps, no status corrections needed",
                recovered_steps.len()
            );
        }

        Ok(corrections_made)
    }

    /// Execute the workflow to completion using parallel execution.
    /// This method runs until all steps are completed and returns the final result.
    pub async fn execute_to_completion(&mut self) -> Result<FlowResult> {
        let mut running_tasks = FuturesUnordered::new();

        tracing::debug!("Starting execution of {} steps", self.flow.steps().len());

        // Start initial unblocked steps
        let initial_unblocked = self.tracker.unblocked_steps();
        tracing::debug!(
            "Initially runnable steps: [{}]",
            initial_unblocked
                .iter()
                .map(|idx| self.flow.step(idx).id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );

        self.start_unblocked_steps(&initial_unblocked, &mut running_tasks)
            .await?;

        // Process task completions as they arrive
        while let Some((completed_step_index, step_result)) = running_tasks.next().await {
            let step_result = step_result?;

            // Update tracker and store result
            let newly_unblocked = self.tracker.complete_step(completed_step_index);

            // Record the completed result in the state store (non-blocking)
            let step_id = &self.flow.step(completed_step_index).id;
            tracing::debug!(
                "Step {} completed, newly unblocked steps: [{}]",
                step_id,
                newly_unblocked
                    .iter()
                    .map(|idx| self.flow.step(idx).id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            // Cache the step result before writing to state store
            self.write_cache
                .cache_step_result(completed_step_index, step_result.clone())
                .await;

            self.state_store
                .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                    run_id: self.context.run_id(),
                    step_result: StepResult::new(completed_step_index, step_id, step_result),
                })
                .change_context(ExecutionError::StateError)?;

            // Update status of newly unblocked steps (non-blocking)
            if !newly_unblocked.is_empty() {
                // Cache the status updates before writing to state store
                self.write_cache
                    .cache_step_statuses(StepStatus::Runnable, &newly_unblocked)
                    .await;

                self.state_store
                    .queue_write(stepflow_state::StateWriteOperation::UpdateStepStatuses {
                        run_id: self.context.run_id(),
                        status: StepStatus::Runnable,
                        step_indices: newly_unblocked.clone(),
                    })
                    .change_context(ExecutionError::StateError)?;
            }

            // Start newly unblocked steps
            self.start_unblocked_steps(&newly_unblocked, &mut running_tasks)
                .await?;
        }

        // All tasks completed - try to complete the workflow
        self.resolve_workflow_output().await
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
                let runnable = self.tracker.unblocked_steps();
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
            let runnable = self.tracker.unblocked_steps();

            // Check if target step is runnable
            if runnable.contains(target_step_index) {
                break;
            }

            // If no steps are runnable, we can't make progress
            if runnable.is_empty() {
                tracing::warn!("No runnable steps. Unable to progress.");
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
    pub async fn get_step_output(&self, step_id: &str) -> Result<FlowResult> {
        self.resolver
            .resolve_step(step_id)
            .await
            .change_context(ExecutionError::ValueResolverFailure)
            .attach_printable_lazy(|| format!("Failed to get output for step '{step_id}'"))
    }

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
        let runnable = self.tracker.unblocked_steps();

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
                    FlowResult::Skipped => StepStatus::Skipped,
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
            skip_if: step.skip_if.clone(),
            on_error: step.on_error.clone(),
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
        if !self.tracker.unblocked_steps().contains(step_index) {
            return Err(ExecutionError::StepNotRunnable {
                step: step.id.clone(),
            }
            .into());
        }

        // Check skip condition if present
        if let Some(skip_if) = &step.skip_if {
            if self.should_skip_step(skip_if).await? {
                let result = FlowResult::Skipped;
                self.record_step_completion(step_index, &result).await?;
                return Ok(StepExecutionResult::new(
                    step_index,
                    step_id,
                    component_string,
                    result,
                ));
            }
        }

        // Resolve step inputs
        let step_input = match self
            .resolver
            .resolve_template(&step.input)
            .await
            .change_context(ExecutionError::ValueResolverFailure)?
        {
            FlowResult::Success(result) => result,
            FlowResult::Skipped => {
                // Step inputs contain skipped values - skip this step
                let result = FlowResult::Skipped;
                self.record_step_completion(step_index, &result).await?;
                return Ok(StepExecutionResult::new(
                    step_index,
                    step_id,
                    component_string,
                    result,
                ));
            }
            FlowResult::Failed(error) => {
                return Ok(StepExecutionResult::new(
                    step_index,
                    step_id,
                    component_string,
                    FlowResult::Failed(error),
                ));
            }
        };

        // Get plugin and resolved component name
        let (plugin, resolved_component) = self
            .executor
            .get_plugin_and_component(&step.component, step_input.clone())
            .await?;
        let result = execute_step_async(
            plugin,
            step,
            &resolved_component,
            step_input,
            self.context.clone(),
            &self.resolver,
        )
        .await?;

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
        // Update dependency tracker
        self.tracker.complete_step(step_index);

        // Cache the step result before writing to state store
        self.write_cache
            .cache_step_result(step_index, result.clone())
            .await;

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
    pub async fn resolve_workflow_output(&self) -> Result<FlowResult> {
        self.resolver
            .resolve_template(self.flow.output())
            .await
            .change_context(ExecutionError::ValueResolverFailure)
    }

    /// Get access to the state store for querying step results.
    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        &self.state_store
    }

    /// Check if a step should be skipped based on its skip condition.
    ///
    /// Evaluates the skip_if expression and returns true if the step should be skipped.
    /// - If the expression resolves to a truthy value, the step is skipped
    /// - If the expression references skipped steps or fails, the step is NOT skipped
    ///   (allowing the step to potentially handle the skip/error via on_skip logic)
    async fn should_skip_step(&self, skip_if: &Expr) -> Result<bool> {
        let resolved_value = self
            .resolver
            .resolve_expr(skip_if)
            .await
            .change_context(ExecutionError::ValueResolverFailure)?;

        match resolved_value {
            FlowResult::Success(result) => Ok(result.is_truthy()),
            FlowResult::Skipped => Ok(false), // Don't skip if condition references skipped values
            FlowResult::Failed { .. } => Ok(false), // Don't skip if condition evaluation failed
        }
    }

    /// Start all newly unblocked steps, handling skips and starting executions.
    ///
    /// This method implements a "fast skip" optimization where skip conditions and input-based
    /// skips are evaluated synchronously before spawning async tasks. This avoids the overhead
    /// of creating async tasks for steps that will immediately skip, and handles skip chains
    /// (A skips → B skips → C skips) efficiently in a single loop.
    ///
    /// The loop continues until no more steps can be fast-skipped, ensuring that cascading
    /// skips are processed without waiting for async execution cycles.
    async fn start_unblocked_steps(
        &mut self,
        unblocked: &BitSet,
        running_tasks: &mut FuturesUnordered<BoxFuture<'static, (usize, Result<FlowResult>)>>,
    ) -> Result<()> {
        let mut steps_to_process = unblocked.clone();

        // Fast skip loop: process chains of skippable steps synchronously
        // This avoids spawning async tasks for steps that will immediately skip
        while !steps_to_process.is_empty() {
            let mut additional_unblocked = BitSet::new();

            for step_index in steps_to_process.iter() {
                // Extract step data to avoid borrowing issues
                let (step_id, skip_if, step_input) = {
                    let step = &self.flow.step(step_index);
                    (step.id.clone(), step.skip_if.clone(), step.input.clone())
                };

                // Check explicit skip condition (skip_if expression)
                if let Some(skip_if) = &skip_if {
                    if self.should_skip_step(skip_if).await? {
                        // Skip this step and collect any newly unblocked dependent steps
                        additional_unblocked
                            .union_with(&self.skip_step(&step_id, step_index).await?);
                        continue;
                    }
                }

                // Check for input-based skips: if any input references a skipped step,
                // this step should also be skipped (unless using on_skip with use_default)
                let step_input = self
                    .resolver
                    .resolve_template(&step_input)
                    .await
                    .change_context(ExecutionError::ValueResolverFailure)?;
                let step_input = match step_input {
                    FlowResult::Success(result) => result,
                    FlowResult::Skipped => {
                        // Step inputs contain skipped values - propagate the skip
                        additional_unblocked
                            .union_with(&self.skip_step(&step_id, step_index).await?);
                        continue;
                    }
                    FlowResult::Failed(error) => {
                        tracing::error!(
                            "Failed to resolve inputs for step {} - input resolution failed: {:?}",
                            step_id,
                            error
                        );
                        return Err(ExecutionError::StepFailed { step: step_id }.into());
                    }
                };

                // Step passed all skip checks - start async execution
                self.start_step_execution(step_index, step_input, running_tasks)
                    .await?;
            }

            // Process any steps that were unblocked by skips in this iteration
            // This enables skip chains: A skips → B skips → C skips
            steps_to_process = additional_unblocked;
        }

        Ok(())
    }

    /// Skip a step and record the result.
    async fn skip_step(&mut self, step_id: &str, step_index: usize) -> Result<BitSet> {
        tracing::debug!("Skipping step {} at index {}", step_id, step_index);

        let newly_unblocked_from_skip = self.tracker.complete_step(step_index);
        let skip_result = FlowResult::Skipped;

        // Cache the skipped result before writing to state store
        self.write_cache
            .cache_step_result(step_index, skip_result.clone())
            .await;

        // Record the skipped result in the state store (non-blocking)
        if let Err(e) =
            self.state_store
                .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                    run_id: self.context.run_id(),
                    step_result: StepResult::new(step_index, step_id, skip_result),
                })
        {
            tracing::error!("Failed to queue step result: {:?}", e);
        }

        tracing::debug!(
            "Step {} skipped, newly unblocked steps: [{}]",
            step_id,
            newly_unblocked_from_skip
                .iter()
                .map(|idx| self.flow.step(idx).id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );

        Ok(newly_unblocked_from_skip)
    }

    /// Start asynchronous execution of a step.
    async fn start_step_execution(
        &self,
        step_index: usize,
        step_input: ValueRef,
        running_tasks: &mut FuturesUnordered<BoxFuture<'static, (usize, Result<FlowResult>)>>,
    ) -> Result<()> {
        let step = self.flow.step(step_index);
        tracing::debug!("Starting execution of step {}", step.id);

        // Get plugin and resolved component name for this step
        let (plugin, resolved_component) = self
            .executor
            .get_plugin_and_component(&step.component, step_input.clone())
            .await?;

        // Clone necessary data for the async task
        let flow = self.flow.clone();
        let context = self.context.clone();
        let resolver = self.resolver.clone();

        // Create the async task
        let plugin_clone = plugin.clone();
        let resolved_component_clone = resolved_component.clone();
        let task_future: BoxFuture<'static, (usize, Result<FlowResult>)> = Box::pin(async move {
            let step = flow.step(step_index);
            let result = execute_step_async(
                &plugin_clone,
                step,
                &resolved_component_clone,
                step_input,
                context,
                &resolver,
            )
            .await;
            (step_index, result)
        });

        running_tasks.push(task_future);

        Ok(())
    }
}

/// Execute a single step asynchronously.
pub(crate) async fn execute_step_async(
    plugin: &Arc<DynPlugin<'static>>,
    step: &stepflow_core::workflow::Step,
    resolved_component: &str,
    input: ValueRef,
    context: ExecutionContext,
    resolver: &ValueResolver<StateValueLoader>,
) -> Result<FlowResult> {
    // Create a component from the resolved component name
    let component = stepflow_core::workflow::Component::from_string(resolved_component);

    // Execute the component
    let result = plugin
        .execute(&component, context, input)
        .await
        .change_context(ExecutionError::StepFailed {
            step: step.id.to_owned(),
        })?;

    match &result {
        FlowResult::Failed(error) => {
            match &step.on_error {
                stepflow_core::workflow::ErrorAction::Skip => {
                    tracing::debug!(
                        "Step {} failed but configured to skip: {:?}",
                        step.id,
                        error
                    );
                    Ok(FlowResult::Skipped)
                }
                stepflow_core::workflow::ErrorAction::UseDefault { default_value } => {
                    tracing::debug!(
                        "Step {} failed but using default value: {:?}",
                        step.id,
                        error
                    );
                    let template = match default_value {
                        Some(default) => default.clone(),
                        None => ValueTemplate::null(),
                    };
                    // Resolve the ValueTemplate to get the actual value
                    let default_value = resolver
                        .resolve_template(&template)
                        .await
                        .change_context(ExecutionError::ValueResolverFailure)?;
                    match default_value {
                        FlowResult::Success(result) => Ok(FlowResult::Success(result)),
                        FlowResult::Skipped => {
                            // Default value resolved to skipped - treat as null
                            Ok(FlowResult::Success(ValueRef::new(serde_json::Value::Null)))
                        }
                        FlowResult::Failed(error) => {
                            error_stack::bail!(ExecutionError::internal(format!(
                                "Default value resolution failed for step {}: {:?}",
                                step.id, error
                            )))
                        }
                    }
                }
                stepflow_core::workflow::ErrorAction::Fail => Ok(result),
                stepflow_core::workflow::ErrorAction::Retry => {
                    // TODO: Implement retry logic
                    tracing::warn!(
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
    pub input: ValueTemplate,
    pub skip_if: Option<Expr>,
    pub on_error: stepflow_core::workflow::ErrorAction,
    pub state: StepStatus,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use serde_json::json;
    use stepflow_core::FlowError;
    use stepflow_core::workflow::{Component, ErrorAction, Flow, FlowV1, Step, StepId};
    use stepflow_mock::{MockComponentBehavior, MockPlugin};
    use stepflow_state::InMemoryStateStore;

    /// Helper function to create workflow from YAML string with simple mock behaviors
    /// Each component gets a single behavior that will be used regardless of input
    pub async fn create_workflow_from_yaml_simple(
        yaml_str: &str,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> (Arc<crate::executor::StepFlowExecutor>, Arc<Flow>, BlobId) {
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

        let executor = crate::executor::StepFlowExecutor::new(
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
        let run_id = Uuid::new_v4();
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let input_ref = ValueRef::new(input);

        execute_workflow(executor, flow, flow_id, run_id, input_ref, state_store).await
    }

    /// Create a WorkflowExecutor from YAML string for step-by-step testing
    pub async fn create_workflow_executor_from_yaml_simple(
        yaml_str: &str,
        input: serde_json::Value,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> Result<WorkflowExecutor> {
        let (executor, flow, flow_id) =
            create_workflow_from_yaml_simple(yaml_str, mock_behaviors).await;
        let run_id = Uuid::new_v4();
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let input_ref = ValueRef::new(input);

        WorkflowExecutor::new(executor, flow, flow_id, run_id, input_ref, state_store)
    }

    fn create_test_step(id: &str, input: serde_json::Value) -> Step {
        Step {
            id: id.to_string(),
            component: Component::from_string("/mock/test"),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::Fail,
            input: ValueTemplate::parse_value(input).unwrap(),
        }
    }

    fn create_test_flow(steps: Vec<Step>, output: ValueTemplate) -> Flow {
        Flow::V1(FlowV1 {
            name: None,
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps,
            output,
            test: None,
            examples: None,
        })
    }

    #[tokio::test]
    async fn test_dependency_tracking_basic() {
        // Test that we can create dependencies and tracker correctly
        let steps = vec![
            create_test_step("step1", json!({"value": 42})),
            create_test_step("step2", json!({"$from": {"step": "step1"}})),
        ];

        let flow = Arc::new(create_test_flow(
            steps,
            ValueTemplate::parse_value(json!({"$from": {"step": "step2"}})).unwrap(),
        ));

        // Build dependencies using analysis crate
        let analysis_result = stepflow_analysis::analyze_flow_dependencies(
            flow.clone(),
            BlobId::new("a".repeat(64)).unwrap(),
        )
        .unwrap();

        // Create tracker from analysis
        let analysis = analysis_result.analysis.unwrap();
        let tracker = analysis.new_dependency_tracker();

        // Only step1 should be runnable initially
        let unblocked = tracker.unblocked_steps();
        assert_eq!(unblocked.len(), 1);
        assert!(unblocked.contains(0)); // step1

        // This confirms the tracker integration is working
    }

    #[tokio::test]
    async fn test_simple_workflow_execution() {
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: step1
    component: /mock/simple
    input:
      $from:
        workflow: input
output:
  $from:
    step: step1
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
      $from:
        workflow: input
  - id: step2
    component: /mock/second
    input:
      $from:
        step: step1
output:
  $from:
    step: step2
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
      $from:
        workflow: input
  - id: step2
    component: /mock/second
    input:
      $from:
        step: step1
output:
  $from:
    step: step2
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
        let final_result = workflow_executor.resolve_workflow_output().await.unwrap();
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
      value: { $from: { step: step1 } }
output:
  final: { $from: { step: step2 } }
"#;

        let workflow: Arc<Flow> = Arc::new(serde_yaml_ng::from_str(workflow_yaml).unwrap());
        let flow_id = BlobId::from_flow(&workflow).unwrap();
        let executor = StepFlowExecutor::new_in_memory();
        let run_id = Uuid::new_v4();
        let input = ValueRef::new(json!({}));

        // Create a workflow executor
        let mut workflow_executor = WorkflowExecutor::new(
            executor.clone(),
            workflow.clone(),
            flow_id.clone(),
            run_id,
            input.clone(),
            executor.state_store(),
        )
        .unwrap();

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

        // Verify initial state: step2 should not be runnable in the tracker yet
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

        // Verify that step1 result is cached
        let cached_result = workflow_executor
            .write_cache
            .get_step_result(&StepId {
                index: 0,
                flow: workflow_executor.flow().clone(),
            })
            .await;
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
      value: { $from: { step: step1 } }
  - id: step3
    component: /mock/identity
    input:
      value: { $from: { step: step2 } }
output:
  final: { $from: { step: step3 } }
"#;

        let workflow: Arc<Flow> = Arc::new(serde_yaml_ng::from_str(workflow_yaml).unwrap());
        let flow_id = BlobId::from_flow(&workflow).unwrap();
        let executor = StepFlowExecutor::new_in_memory();
        let run_id = Uuid::new_v4();
        let input = ValueRef::new(json!({}));

        let workflow_executor = WorkflowExecutor::new(
            executor.clone(),
            workflow.clone(),
            flow_id.clone(),
            run_id,
            input.clone(),
            executor.state_store(),
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
      value: { $from: { step: step1 } }
  - id: step4
    component: /mock/identity
    input:
      value: { $from: { step: step2 } }
  - id: step5
    component: /mock/identity
    input:
      deps: [{ $from: { step: step3 } }, { $from: { step: step4 } }]
output:
  final: { $from: { step: step5 } }
"#;

        let workflow: Arc<Flow> = Arc::new(serde_yaml_ng::from_str(workflow_yaml).unwrap());
        let flow_id = BlobId::from_flow(&workflow).unwrap();
        let executor = StepFlowExecutor::new_in_memory();
        let run_id = Uuid::new_v4();
        let input = ValueRef::new(json!({}));

        let mut workflow_executor = WorkflowExecutor::new(
            executor.clone(),
            workflow.clone(),
            flow_id.clone(),
            run_id,
            input.clone(),
            executor.state_store(),
        )
        .unwrap();

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

        // Before recovery: only step1 and step2 should be runnable (fresh tracker)
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
  final: { $from: { step: step1 } }
"#;

        let workflow: Arc<Flow> = Arc::new(serde_yaml_ng::from_str(workflow_yaml).unwrap());
        let flow_id = BlobId::from_flow(&workflow).unwrap();
        let executor = StepFlowExecutor::new_in_memory();
        let run_id = Uuid::new_v4();
        let input = ValueRef::new(json!({}));

        let mut workflow_executor = WorkflowExecutor::new(
            executor.clone(),
            workflow.clone(),
            flow_id.clone(),
            run_id,
            input.clone(),
            executor.state_store(),
        )
        .unwrap();

        // Test with a fresh execution - since there are no completed steps and no step info
        // in the database, recovery should mark the initially runnable step as Runnable
        let corrections_made = workflow_executor.recover_from_state_store().await.unwrap();

        // Since step1 should be runnable but there's no status info, recovery should fix it
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
      $from:
        workflow: input
  - id: step2
    component: /mock/parallel2
    input:
      $from:
        workflow: input
  - id: final
    component: /mock/combiner
    input:
      step1:
        $from:
          step: step1
      step2:
        $from:
          step: step2
output:
  $from:
    step: final
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
        let sequential_result = sequential_executor.resolve_workflow_output().await.unwrap();

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
      action: skip
    input:
      mode: error
output:
  $from:
    step: failing_step
"#;

        let mock_behaviors = vec![(
            "/mock/error",
            FlowResult::Failed(FlowError::new(500, "Test error")),
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // The workflow should complete with skipped result
        match result {
            FlowResult::Skipped => {
                // Expected - the step failed but was configured to skip
            }
            _ => panic!("Expected skipped result, got: {result:?}"),
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
  $from:
    step: failing_step
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
  $from:
    step: failing_step
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
  $from:
    step: failing_step
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
      action: skip
    input: {}
output:
  $from:
    step: success_step
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
    async fn test_error_handling_skip_with_multi_step() {
        // Test that when a step is skipped, downstream steps handle it correctly
        let workflow_yaml = r#"
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: failing_step
    component: /mock/error
    onError:
      action: skip
    input:
      mode: error
  - id: downstream_step
    component: /mock/success
    input:
      $from:
        step: failing_step
output:
  $from:
    step: downstream_step
"#;

        let mock_behaviors = vec![
            (
                "/mock/error",
                FlowResult::Failed(FlowError::new(500, "Test error")),
            ),
            (
                "/mock/success",
                FlowResult::Success(ValueRef::new(json!({"handled_skip":true}))),
            ),
        ];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // The downstream step should skip because its input depends on a skipped step
        match result {
            FlowResult::Skipped => {
                // Expected - the downstream step should be skipped when its input is skipped
            }
            _ => panic!("Expected skipped result for downstream step, got: {result:?}"),
        }
    }
}
