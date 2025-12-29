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

//! Flow executor for unified single and multi-item workflow execution.
//!
//! [`FlowExecutor`] provides the main execution engine for running workflows
//! with one or more items. It coordinates task execution across items using
//! a pluggable [`Scheduler`] and manages concurrency, state tracking, and
//! result recording.

use std::collections::HashMap;
use std::sync::Arc;

use error_stack::ResultExt as _;
use futures::stream::{FuturesUnordered, StreamExt as _};
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::{Flow, WorkflowOverrides, apply_overrides};
use stepflow_core::{BlobId, FlowResult};
use stepflow_observability::RunInfoGuard;
use stepflow_state::StateStore;
use uuid::Uuid;

use crate::scheduler::Scheduler;
use crate::state::ItemsState;
use crate::step_runner::{StepRunResult, StepRunner};
use crate::task::{Task, TaskResult};
use crate::{ExecutionError, Result};

/// Executor for running workflows with one or more items.
///
/// FlowExecutor coordinates execution across multiple items, using a scheduler
/// to determine task ordering and respecting concurrency limits. It supports
/// both run-to-completion and streaming execution modes.
///
/// # Example
///
/// ```ignore
/// let flow = Arc::new(flow);
/// let inputs = vec![input1, input2, input3];
/// let state = ItemsState::batch(flow.clone(), inputs, variables);
///
/// let mut executor = FlowExecutor::new(
///     stepflow.clone(),
///     run_id,
///     flow_id,
///     state,
///     Box::new(DepthFirstScheduler::new()),
///     10, // max concurrency
///     state_store.clone(),
/// );
///
/// executor.execute_to_completion().await?;
/// let results = state_store
///     .get_item_results(run_id, ResultOrder::ByIndex)
///     .await?;
/// ```
pub struct FlowExecutor {
    /// Reference to the StepflowExecutor for plugin access.
    stepflow: Arc<crate::StepflowExecutor>,
    /// Unique run ID for this execution.
    run_id: Uuid,
    /// Flow ID (content hash) for the workflow.
    flow_id: BlobId,
    /// Per-item execution state.
    state: ItemsState,
    /// Scheduler for task ordering.
    scheduler: Box<dyn Scheduler>,
    /// Maximum concurrent tasks.
    max_in_flight: usize,
    /// State store for persisting results.
    state_store: Arc<dyn StateStore>,
}

impl FlowExecutor {
    /// Create a new items executor.
    ///
    /// # Arguments
    ///
    /// * `stepflow` - Reference to the StepflowExecutor for plugin access
    /// * `run_id` - Unique run ID for this execution
    /// * `flow_id` - Flow ID (content hash) for the workflow
    /// * `state` - Pre-initialized ItemsState with items and variables
    /// * `scheduler` - Scheduler for task ordering
    /// * `max_in_flight` - Maximum concurrent tasks
    /// * `state_store` - State store for persisting results
    pub fn new(
        stepflow: Arc<crate::StepflowExecutor>,
        run_id: Uuid,
        flow_id: BlobId,
        state: ItemsState,
        scheduler: Box<dyn Scheduler>,
        max_in_flight: usize,
        state_store: Arc<dyn StateStore>,
    ) -> Self {
        Self {
            stepflow,
            run_id,
            flow_id,
            state,
            scheduler,
            max_in_flight,
            state_store,
        }
    }

    /// Execute tasks until idle or fuel is exhausted.
    ///
    /// # Arguments
    ///
    /// * `fuel` - Maximum number of tasks to complete before returning.
    ///   - `None`: Run until scheduler returns Idle and all in-flight complete
    ///   - `Some(n)`: Complete at most `n` tasks, then return
    ///
    /// Results are recorded to the state store as tasks complete.
    ///
    /// For single-task execution (debug stepping), use [`run_single_task`] instead.
    pub async fn run(&mut self, fuel: Option<std::num::NonZeroUsize>) -> Result<()> {
        let mut in_flight: FuturesUnordered<futures::future::BoxFuture<'static, TaskResult>> =
            FuturesUnordered::new();
        let mut remaining = fuel.map(|f| f.get());

        loop {
            // Check if we've exhausted fuel
            if remaining == Some(0) {
                // Wait for any in-flight tasks to complete before returning
                while let Some(task_result) = in_flight.next().await {
                    self.complete_task(task_result);
                }
                return Ok(());
            }

            // Determine how many more tasks we can start
            let max_to_start = {
                let max_to_start = self.max_in_flight.saturating_sub(in_flight.len());
                if let Some(r) = remaining {
                    max_to_start.min(r)
                } else {
                    max_to_start
                }
            };

            // Try to start new tasks
            if max_to_start > 0
                && let Some(tasks) = self.scheduler.select_next(max_to_start).into_tasks()
            {
                for task in tasks.into_iter() {
                    self.state.mark_executing(task);
                    let future = self.prepare_task_future(task)?;
                    in_flight.push(future);
                }
            }

            // If no tasks in flight, we're done
            if in_flight.is_empty() {
                return Ok(());
            }

            // Wait for any task to complete
            if let Some(task_result) = in_flight.next().await {
                self.complete_task(task_result);
                if let Some(r) = &mut remaining {
                    *r = r.saturating_sub(1);
                }
            }
        }
    }

    /// Execute a single ready task and return its result.
    ///
    /// This is optimized for debug mode where we want exactly one result
    /// without the overhead of the general `run()` machinery.
    ///
    /// Returns `None` if no tasks are ready.
    pub async fn run_single_task(&mut self) -> Result<Option<StepRunResult>> {
        // Get one task from scheduler
        let Some(tasks) = self.scheduler.select_next(1).into_tasks() else {
            return Ok(None);
        };

        // Execute the single task
        let task = tasks.into_iter().next().unwrap();
        self.state.mark_executing(task);
        let future = self.prepare_task_future(task)?;
        let task_result = future.await;

        // Complete and return
        let step_result = task_result.step.clone();
        self.complete_task(task_result);
        Ok(Some(step_result))
    }

    /// Execute all items to completion.
    ///
    /// This method drives execution until all items complete, then:
    /// 1. Records item results to the state store
    /// 2. Updates the run status (Completed or Failed)
    ///
    /// Use `state_store.get_item_results(run_id, order)` after completion
    /// to retrieve the results.
    ///
    /// Tasks are executed concurrently up to `max_in_flight` at a time.
    pub async fn execute_to_completion(&mut self) -> Result<()> {
        // Set run_id in diagnostic context
        let _run_guard = RunInfoGuard::new(self.flow_id.to_string(), self.run_id.to_string());

        log::info!(
            "Starting items execution: run_id={}, item_count={}, max_in_flight={}",
            self.run_id,
            self.state.item_count(),
            self.max_in_flight
        );

        // Initialize all items and get initial ready tasks
        self.scheduler.reset();
        let initial_tasks = self.state.initialize_all();
        self.scheduler.notify_new_tasks(&initial_tasks);

        // Run until complete or deadlock
        self.run(None).await?;

        // Check for deadlock: run() returned but not complete
        if !self.state.is_all_complete() {
            return Err(error_stack::report!(ExecutionError::Deadlock)
                .attach_printable("No tasks ready and none in flight"));
        }

        // Record item results and determine final status
        let mut has_failures = false;
        for item_index in 0..self.state.item_count() {
            let result = self.resolve_item_output(item_index);
            if matches!(&result, FlowResult::Failed(_)) {
                has_failures = true;
            }
            // Record to state store (ignore errors - best effort)
            let _ = self
                .state_store
                .record_item_result(self.run_id, item_index as usize, result)
                .await;
        }

        // Update run status
        let final_status = if has_failures {
            stepflow_core::status::ExecutionStatus::Failed
        } else {
            stepflow_core::status::ExecutionStatus::Completed
        };
        let _ = self
            .state_store
            .update_run_status(self.run_id, final_status)
            .await;

        log::info!(
            "Items execution completed: run_id={}, item_count={}, status={:?}",
            self.run_id,
            self.state.item_count(),
            final_status
        );

        Ok(())
    }

    /// Prepare a task for execution and return an owned future.
    ///
    /// This uses [`StepRunner`] to handle plugin lookup, context creation,
    /// and step execution. The future is fully owned and can be polled
    /// without borrowing self.
    fn prepare_task_future(
        &self,
        task: Task,
    ) -> Result<futures::future::BoxFuture<'static, TaskResult>> {
        use futures::future::FutureExt as _;

        // Get flow and resolve step input while we have access to state
        let item = self.state.item(task.item_index);
        let flow = item.flow().clone();
        let step = flow.step(task.step_index);
        let step_input = step.input.resolve(item);

        // Capture step metadata for error handling
        let step = flow.step(task.step_index);
        let step_index = task.step_index;
        let step_id = step.id.clone();
        let component = step.component.to_string();

        // Create step runner with all execution context
        let runner = StepRunner::new(
            flow,
            task.step_index,
            step_input,
            self.stepflow.clone(),
            self.run_id,
            self.flow_id.clone(),
        );

        // Create an owned future that runs the step and handles errors
        let future = async move {
            let step_result = match runner.run().await {
                Ok(result) => result,
                Err(error) => {
                    // Infrastructure error - log and convert to FlowResult::Failed
                    log::error!("Internal error executing step '{}': {:?}", step_id, error);
                    let flow_error =
                        stepflow_core::FlowError::from_error_stack(error.attach_printable(
                            format!("Internal error executing step '{}'", step_id),
                        ));
                    StepRunResult::new(
                        step_index,
                        step_id,
                        component,
                        FlowResult::Failed(flow_error),
                    )
                }
            };
            TaskResult::new(task, step_result)
        };

        Ok(future.boxed())
    }

    /// Complete a task and update state.
    fn complete_task(&mut self, task_result: TaskResult) {
        let task = task_result.task();
        let result = task_result.step.result.clone();

        // Update state and get newly ready tasks
        let new_tasks = self.state.complete_task_and_get_ready(task, result.clone());

        // Notify scheduler
        self.scheduler.task_completed(task);
        if !new_tasks.is_empty() {
            self.scheduler.notify_new_tasks(&new_tasks);
        }

        // Record result to state store using metadata from StepRunResult
        let step_result = stepflow_state::StepResult::new(
            task_result.step_index(),
            task_result.step.step_id(),
            result,
        );
        let _ =
            self.state_store
                .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                    run_id: self.run_id,
                    step_result,
                });
    }

    /// Resolve the output for a completed item.
    fn resolve_item_output(&self, item_index: u32) -> FlowResult {
        let item = self.state.item(item_index);
        let flow = item.flow();
        flow.output().resolve(item)
    }

    /// Record final item result to state store.
    pub async fn record_item_result(&self, item_index: u32, result: FlowResult) -> Result<()> {
        self.state_store
            .record_item_result(self.run_id, item_index as usize, result)
            .await
            .change_context(ExecutionError::StateError)?;
        Ok(())
    }

    /// Get the run ID for this execution.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Get the number of items.
    pub fn item_count(&self) -> u32 {
        self.state.item_count()
    }

    /// Check if all items have completed.
    pub fn is_complete(&self) -> bool {
        self.state.is_all_complete()
    }

    /// Get read access to the execution state.
    pub fn state(&self) -> &ItemsState {
        &self.state
    }

    /// Recover a completed step from persistent storage.
    ///
    /// This is used during session recovery to restore step results.
    /// It properly tracks the incomplete counter.
    pub fn recover_step(&mut self, item_index: u32, step_index: usize, result: FlowResult) {
        // First ensure the step is in the needed set (with proper counter tracking)
        self.state.add_needed(item_index, step_index);

        // Then mark as completed (which will update the counter)
        let item = self.state.item_mut(item_index);
        item.mark_completed(step_index, result);
    }

    /// Get the flow for an item.
    pub fn flow(&self, item_index: u32) -> Arc<Flow> {
        self.state.item(item_index).flow().clone()
    }

    /// Add a step to the needed set for an item.
    ///
    /// This is the primary method for debug mode's "task addition control".
    /// It marks the step and its dependencies as needed, then notifies the
    /// scheduler of any newly ready tasks.
    ///
    /// Returns the step indices that were newly added to the needed set.
    pub fn add_needed(&mut self, item_index: u32, step_index: usize) -> Vec<usize> {
        let newly_needed = self.state.add_needed(item_index, step_index);

        // Notify scheduler of ready tasks
        if !newly_needed.is_empty() {
            let item = self.state.item(item_index);
            let ready_steps = item.ready_steps();
            let ready_tasks: Vec<Task> = newly_needed
                .iter()
                .filter(|idx| ready_steps.contains(**idx))
                .map(|idx| Task::new(item_index, *idx))
                .collect();
            self.scheduler.notify_new_tasks(&ready_tasks);
        }

        newly_needed
    }
}

/// Builder for creating an FlowExecutor with common configurations.
pub struct FlowExecutorBuilder {
    stepflow: Arc<crate::StepflowExecutor>,
    run_id: Option<Uuid>,
    flow: Arc<Flow>,
    flow_id: BlobId,
    inputs: Vec<ValueRef>,
    variables: HashMap<String, ValueRef>,
    overrides: Option<WorkflowOverrides>,
    scheduler: Option<Box<dyn Scheduler>>,
    max_concurrency: usize,
    state_store: Arc<dyn StateStore>,
    skip_validation: bool,
}

impl FlowExecutorBuilder {
    /// Create a new builder.
    pub fn new(
        stepflow: Arc<crate::StepflowExecutor>,
        flow: Arc<Flow>,
        flow_id: BlobId,
        state_store: Arc<dyn StateStore>,
    ) -> Self {
        Self {
            stepflow,
            run_id: None,
            flow,
            flow_id,
            inputs: Vec::new(),
            variables: HashMap::new(),
            overrides: None,
            scheduler: None,
            max_concurrency: 10,
            state_store,
            skip_validation: false,
        }
    }

    /// Skip workflow validation during build.
    ///
    /// Use this when resuming an execution where the flow was already validated,
    /// or when validation was performed externally.
    pub fn skip_validation(mut self) -> Self {
        self.skip_validation = true;
        self
    }

    /// Set the run ID (default: generate new UUID).
    pub fn run_id(mut self, run_id: Uuid) -> Self {
        self.run_id = Some(run_id);
        self
    }

    /// Set a single input (convenience for single-item runs).
    pub fn input(mut self, input: ValueRef) -> Self {
        self.inputs = vec![input];
        self
    }

    /// Set multiple inputs for batch execution.
    pub fn inputs(mut self, inputs: Vec<ValueRef>) -> Self {
        self.inputs = inputs;
        self
    }

    /// Set workflow variables.
    pub fn variables(mut self, variables: HashMap<String, ValueRef>) -> Self {
        self.variables = variables;
        self
    }

    /// Set workflow overrides.
    pub fn overrides(mut self, overrides: WorkflowOverrides) -> Self {
        self.overrides = Some(overrides);
        self
    }

    /// Set the scheduler (default: DepthFirstScheduler).
    pub fn scheduler(mut self, scheduler: Box<dyn Scheduler>) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    /// Set maximum concurrency (default: 10).
    pub fn max_concurrency(mut self, max: usize) -> Self {
        self.max_concurrency = max;
        self
    }

    /// Build the executor.
    ///
    /// This validates the workflow (unless `skip_validation()` was called) and
    /// ensures a run record exists in the state store (idempotent).
    pub async fn build(self) -> Result<FlowExecutor> {
        // Validate workflow unless skipped
        if !self.skip_validation {
            let diagnostics = stepflow_analysis::validate(&self.flow)
                .change_context(ExecutionError::AnalysisError)?;

            if diagnostics.has_fatal() {
                let fatal = diagnostics.num_fatal;
                let error = diagnostics.num_error;
                return Err(error_stack::report!(ExecutionError::AnalysisError)
                    .attach_printable(format!(
                        "Workflow validation failed with {fatal} fatal and {error} error diagnostics"
                    )));
            }
        }

        let run_id = self.run_id.unwrap_or_else(Uuid::now_v7);

        // Apply overrides if provided
        let flow = if let Some(overrides) = &self.overrides {
            apply_overrides(self.flow.clone(), overrides)
                .change_context(ExecutionError::OverrideError)?
        } else {
            self.flow.clone()
        };

        // Ensure run record exists (idempotent - no-op if already created)
        let mut run_params =
            stepflow_state::CreateRunParams::new(run_id, self.flow_id.clone(), self.inputs.clone());
        run_params.workflow_name = self.flow.name().map(|s| s.to_string());
        if let Some(ref o) = self.overrides {
            run_params.overrides = o.clone();
        }
        self.state_store
            .create_run(run_params)
            .await
            .change_context(ExecutionError::StateError)?;

        // Create ItemsState
        let state = ItemsState::batch(flow.clone(), self.inputs, self.variables);

        // Default scheduler
        let scheduler = self
            .scheduler
            .unwrap_or_else(|| Box::new(crate::scheduler::DepthFirstScheduler::new()));

        Ok(FlowExecutor::new(
            self.stepflow,
            run_id,
            self.flow_id,
            state,
            scheduler,
            self.max_concurrency,
            self.state_store,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::{BreadthFirstScheduler, DepthFirstScheduler};
    use crate::testing::{MockExecutorBuilder, create_executor_with_behaviors, create_linear_flow};
    use serde_json::json;
    use stepflow_core::ValueExpr;
    use stepflow_core::workflow::StepBuilder;
    use stepflow_state::ResultOrder;

    #[tokio::test]
    async fn test_single_item_execution() {
        let flow = Arc::new(create_linear_flow(2));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let input = ValueRef::new(json!({"x": 1}));

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .input(input)
                .scheduler(Box::new(DepthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_batch_execution() {
        let flow = Arc::new(create_linear_flow(2));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![
            ValueRef::new(json!({"x": 1})),
            ValueRef::new(json!({"x": 2})),
            ValueRef::new(json!({"x": 3})),
        ];

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .inputs(inputs)
                .scheduler(Box::new(DepthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 3);
        for result in &results {
            assert!(matches!(result.result, Some(FlowResult::Success(_))));
        }
    }

    #[tokio::test]
    async fn test_depth_first_vs_breadth_first() {
        // This test verifies that both schedulers produce correct results
        // (ordering differences are tested in scheduler tests)
        let flow = Arc::new(create_linear_flow(2));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![
            ValueRef::new(json!({"x": 1})),
            ValueRef::new(json!({"x": 2})),
        ];

        // Test with depth-first
        let executor_df = MockExecutorBuilder::new().build().await;
        let state_store_df = executor_df.state_store();
        let mut items_executor_df = FlowExecutorBuilder::new(
            executor_df.clone(),
            flow.clone(),
            flow_id.clone(),
            state_store_df.clone(),
        )
        .inputs(inputs.clone())
        .scheduler(Box::new(DepthFirstScheduler::new()))
        .build()
        .await
        .unwrap();

        let run_id_df = items_executor_df.run_id();
        items_executor_df.execute_to_completion().await.unwrap();
        let results_df = state_store_df
            .get_item_results(run_id_df, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Test with breadth-first
        let executor_bf = MockExecutorBuilder::new().build().await;
        let state_store_bf = executor_bf.state_store();
        let mut items_executor_bf = FlowExecutorBuilder::new(
            executor_bf.clone(),
            flow.clone(),
            flow_id,
            state_store_bf.clone(),
        )
        .inputs(inputs)
        .scheduler(Box::new(BreadthFirstScheduler::new()))
        .build()
        .await
        .unwrap();

        let run_id_bf = items_executor_bf.run_id();
        items_executor_bf.execute_to_completion().await.unwrap();
        let results_bf = state_store_bf
            .get_item_results(run_id_bf, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Both should produce same number of results
        assert_eq!(results_df.len(), results_bf.len());
        assert_eq!(results_df.len(), 2);
    }

    #[tokio::test]
    async fn test_error_isolation_between_items() {
        // Test that when one item fails, other items still succeed
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Create behaviors: x=1 fails, x=2 and x=3 succeed
        let behaviors = vec![
            (
                json!({"x": 1}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Item 1 failed")),
            ),
            (
                json!({"x": 2}),
                FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            ),
            (
                json!({"x": 3}),
                FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            ),
        ];

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.state_store();

        let inputs = vec![
            ValueRef::new(json!({"x": 1})), // Will fail
            ValueRef::new(json!({"x": 2})), // Will succeed
            ValueRef::new(json!({"x": 3})), // Will succeed
        ];

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .inputs(inputs)
                .scheduler(Box::new(DepthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // All items should have results
        assert_eq!(results.len(), 3);

        // Item 0 should fail
        assert!(
            matches!(&results[0].result, Some(FlowResult::Failed(e)) if e.message.contains("Item 1 failed"))
        );

        // Items 1 and 2 should succeed
        assert!(matches!(&results[1].result, Some(FlowResult::Success(_))));
        assert!(matches!(&results[2].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_error_isolation_with_breadth_first() {
        // Same test but with breadth-first scheduler to ensure isolation works regardless of order
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let behaviors = vec![
            (
                json!({"x": 1}),
                FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            ),
            (
                json!({"x": 2}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Middle item failed")),
            ),
            (
                json!({"x": 3}),
                FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            ),
        ];

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.state_store();

        let inputs = vec![
            ValueRef::new(json!({"x": 1})), // Will succeed
            ValueRef::new(json!({"x": 2})), // Will fail
            ValueRef::new(json!({"x": 3})), // Will succeed
        ];

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .inputs(inputs)
                .scheduler(Box::new(BreadthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 3);
        assert!(matches!(&results[0].result, Some(FlowResult::Success(_))));
        assert!(matches!(&results[1].result, Some(FlowResult::Failed(_))));
        assert!(matches!(&results[2].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_concurrent_execution_respects_max_in_flight() {
        // Test that execution works with different max_in_flight settings
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Create many items
        let behaviors: Vec<_> = (1..=10)
            .map(|i| {
                (
                    json!({"x": i}),
                    FlowResult::Success(ValueRef::new(json!({"result": i}))),
                )
            })
            .collect();

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.state_store();

        let inputs: Vec<_> = (1..=10).map(|i| ValueRef::new(json!({"x": i}))).collect();

        // Execute with max_in_flight = 5
        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .inputs(inputs)
                .max_concurrency(5)
                .scheduler(Box::new(BreadthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // All items should complete successfully
        assert_eq!(results.len(), 10);
        for result in &results {
            assert!(matches!(result.result, Some(FlowResult::Success(_))));
        }
    }

    #[tokio::test]
    async fn test_concurrent_execution_with_max_in_flight_one() {
        // Test that execution still works with max_in_flight = 1 (sequential)
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let behaviors: Vec<_> = (1..=5)
            .map(|i| {
                (
                    json!({"x": i}),
                    FlowResult::Success(ValueRef::new(json!({"result": i}))),
                )
            })
            .collect();

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.state_store();

        let inputs: Vec<_> = (1..=5).map(|i| ValueRef::new(json!({"x": i}))).collect();

        // Execute with max_in_flight = 1 (effectively sequential)
        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .inputs(inputs)
                .max_concurrency(1)
                .scheduler(Box::new(DepthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // All items should complete successfully
        assert_eq!(results.len(), 5);
        for result in &results {
            assert!(matches!(result.result, Some(FlowResult::Success(_))));
        }
    }

    #[tokio::test]
    async fn test_empty_inputs() {
        // Test with no inputs - should return empty results
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .inputs(vec![])
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_all_items_fail() {
        // Test that all items failing doesn't cause panic
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Create behaviors where all items fail
        let behaviors = vec![
            (
                json!({"x": 1}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Error 1")),
            ),
            (
                json!({"x": 2}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Error 2")),
            ),
            (
                json!({"x": 3}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Error 3")),
            ),
        ];

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.state_store();

        let inputs = vec![
            ValueRef::new(json!({"x": 1})),
            ValueRef::new(json!({"x": 2})),
            ValueRef::new(json!({"x": 3})),
        ];

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .inputs(inputs)
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // All items should have failed results
        assert_eq!(results.len(), 3);
        for result in &results {
            assert!(matches!(result.result, Some(FlowResult::Failed(_))));
        }
    }

    #[tokio::test]
    async fn test_single_step_flow() {
        // Test minimal flow with just one step
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let input = ValueRef::new(json!({"x": 1}));

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .input(input)
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_run_with_fuel_limit() {
        // Test that fuel limits partial execution
        // Use chain flow so steps execute sequentially (each depends on previous)
        use crate::testing::create_chain_flow;

        let flow = Arc::new(create_chain_flow(3));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let input = ValueRef::new(json!({"x": 1}));

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .input(input)
                .scheduler(Box::new(DepthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        // Initialize and get initial tasks
        items_executor.scheduler.reset();
        let initial_tasks = items_executor.state.initialize_all();
        items_executor.scheduler.notify_new_tasks(&initial_tasks);

        // Run with fuel=1 - should complete exactly 1 task
        items_executor
            .run(Some(std::num::NonZeroUsize::new(1).unwrap()))
            .await
            .unwrap();

        // Should not be complete yet (only 1 of 3 steps done)
        assert!(!items_executor.is_complete());

        // Run with fuel=1 again
        items_executor
            .run(Some(std::num::NonZeroUsize::new(1).unwrap()))
            .await
            .unwrap();

        // Still not complete (2 of 3 steps done)
        assert!(!items_executor.is_complete());

        // Run with fuel=None to complete
        items_executor.run(None).await.unwrap();

        // Now should be complete
        assert!(items_executor.is_complete());
    }

    #[tokio::test]
    async fn test_run_single_task() {
        // Use chain flow so steps execute sequentially (each depends on previous)
        use crate::testing::create_chain_flow;

        let flow = Arc::new(create_chain_flow(3));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let input = ValueRef::new(json!({"x": 1}));

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .input(input)
                .scheduler(Box::new(DepthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        // Initialize state
        items_executor.scheduler.reset();
        let initial_tasks = items_executor.state.initialize_all();
        items_executor.scheduler.notify_new_tasks(&initial_tasks);

        // Run single task
        let result = items_executor.run_single_task().await.unwrap();
        assert!(result.is_some());
        let step_result = result.unwrap();
        assert!(matches!(step_result.result, FlowResult::Success(_)));

        // Run another single task
        let result = items_executor.run_single_task().await.unwrap();
        assert!(result.is_some());

        // Run third task
        let result = items_executor.run_single_task().await.unwrap();
        assert!(result.is_some());

        // No more tasks
        let result = items_executor.run_single_task().await.unwrap();
        assert!(result.is_none());

        // Should be complete
        assert!(items_executor.is_complete());
    }

    #[tokio::test]
    async fn test_chain_flow_dependencies() {
        // Test a flow where steps depend on previous steps
        use crate::testing::create_chain_flow;

        let flow = Arc::new(create_chain_flow(3));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let input = ValueRef::new(json!({"x": 1}));

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .input(input)
                .scheduler(Box::new(DepthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_diamond_flow_parallel_execution() {
        // Test diamond DAG: A → B, A → C, B+C → D
        // B and C should be able to execute in parallel after A completes
        use crate::testing::create_diamond_flow;

        let flow = Arc::new(create_diamond_flow());
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Create behaviors for each step input
        let behaviors = vec![
            // Step A receives input
            (
                json!({"x": 1}),
                FlowResult::Success(ValueRef::new(json!({"a": "result"}))),
            ),
            // Steps B and C receive A's output
            (
                json!({"a": "result"}),
                FlowResult::Success(ValueRef::new(json!({"bc": "result"}))),
            ),
            // Step D receives object with B and C results
            (
                json!({"b": {"bc": "result"}, "c": {"bc": "result"}}),
                FlowResult::Success(ValueRef::new(json!({"d": "final"}))),
            ),
        ];

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.state_store();

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .input(ValueRef::new(json!({"x": 1})))
                .max_concurrency(2) // Allow parallel execution of B and C
                .scheduler(Box::new(BreadthFirstScheduler::new()))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_validation_failure() {
        // Test that validation failures are caught by FlowExecutorBuilder
        use stepflow_core::workflow::FlowBuilder;

        // Create an invalid flow with circular dependency
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Step {
                            step: "step2".to_string(), // depends on step2
                            path: Default::default(),
                        })
                        .build(),
                    StepBuilder::new("step2")
                        .component("/mock/test")
                        .input(ValueExpr::Step {
                            step: "step1".to_string(), // depends on step1 - circular!
                            path: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step2".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        // Build should fail due to validation
        let result = FlowExecutorBuilder::new(executor, flow, flow_id, state_store)
            .input(ValueRef::new(json!({})))
            .build()
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_skip_validation() {
        // Test that skip_validation() bypasses validation
        use stepflow_core::workflow::FlowBuilder;

        // Create an invalid flow
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Step {
                            step: "step2".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                    StepBuilder::new("step2")
                        .component("/mock/test")
                        .input(ValueExpr::Step {
                            step: "step1".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step2".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        // Build should succeed with skip_validation
        let result = FlowExecutorBuilder::new(executor, flow, flow_id, state_store)
            .input(ValueRef::new(json!({})))
            .skip_validation()
            .build()
            .await;

        // Build succeeds (validation skipped)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_variables_in_step_input() {
        // Test that workflow variables are properly resolved in step inputs
        use crate::testing::MockExecutorBuilder;
        use stepflow_core::workflow::{FlowBuilder, JsonPath};

        // Create a flow where step input uses a variable
        // Step input: { "api_key": { "$variable": "api_key" }, "data": { "$input": "" } }
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Object(vec![
                            (
                                "api_key".to_string(),
                                ValueExpr::Variable {
                                    variable: JsonPath::from("api_key"),
                                    default: None,
                                },
                            ),
                            (
                                "data".to_string(),
                                ValueExpr::Input {
                                    input: Default::default(),
                                },
                            ),
                        ]))
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Mock executor that expects the resolved input with the variable value
        let expected_input = json!({"api_key": "secret123", "data": {"x": 1}});
        let executor = MockExecutorBuilder::new()
            .with_input(expected_input.clone())
            .with_success_result(json!({"processed": true}))
            .build()
            .await;
        let state_store = executor.state_store();

        // Create variables map
        let mut variables = HashMap::new();
        variables.insert("api_key".to_string(), ValueRef::new(json!("secret123")));

        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .input(ValueRef::new(json!({"x": 1})))
                .variables(variables)
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Step should have executed successfully with resolved variable
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_variable_with_default() {
        // Test that variable defaults are used when variable is not provided
        use crate::testing::MockExecutorBuilder;
        use stepflow_core::workflow::{FlowBuilder, JsonPath};

        // Create a flow where step input uses a variable with a default
        // { "$variable": "missing_var", "$default": "default_value" }
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Variable {
                            variable: JsonPath::from("missing_var"),
                            default: Some(Box::new(ValueExpr::Literal(json!("default_value")))),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Mock executor expects the default value
        let executor = MockExecutorBuilder::new()
            .with_input(json!("default_value"))
            .with_success_result(json!({"result": "ok"}))
            .build()
            .await;
        let state_store = executor.state_store();

        // No variables provided - should use default
        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .input(ValueRef::new(json!({})))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_variable_missing_fails() {
        // Test that a missing variable without default causes failure
        use crate::testing::MockExecutorBuilder;
        use stepflow_core::workflow::{FlowBuilder, JsonPath};

        // Create a flow where step input uses a variable without default
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Variable {
                            variable: JsonPath::from("missing_var"),
                            default: None, // No default!
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.state_store();

        // No variables provided - should fail on missing variable
        let mut items_executor =
            FlowExecutorBuilder::new(executor, flow.clone(), flow_id, state_store.clone())
                .input(ValueRef::new(json!({})))
                .build()
                .await
                .unwrap();

        let run_id = items_executor.run_id();
        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Step should have failed due to missing variable
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Failed(_))));
    }
}
