use std::sync::Arc;

use bit_set::BitSet;
use futures::{StreamExt as _, future::BoxFuture, stream::FuturesUnordered};
use stepflow_core::{
    FlowResult,
    workflow::{Expr, Flow, ValueRef},
};
use stepflow_plugin::{DynPlugin, ExecutionContext, Plugin as _};
use stepflow_state::{StateStore, StepResult};
use uuid::Uuid;

use crate::{
    ExecutionError, Result, StepFlowExecutor, dependency_analysis::build_dependencies_from_flow,
    tracker::DependencyTracker, value_resolver::ValueResolver,
};

/// Execute a workflow and return the result.
pub(crate) async fn execute_workflow(
    executor: Arc<StepFlowExecutor>,
    flow: Arc<Flow>,
    execution_id: Uuid,
    input: ValueRef,
    state_store: Arc<dyn StateStore>,
) -> Result<FlowResult> {
    // Build dependencies first
    let dependencies = build_dependencies_from_flow(&flow)?;

    // Create the executor with a reference to dependencies
    let workflow_executor = WorkflowExecutor::new(
        executor,
        flow,
        execution_id,
        input,
        state_store,
        dependencies,
    );

    // Execute the workflow
    workflow_executor.execute().await
}

/// Workflow executor that manages the execution of a single workflow
struct WorkflowExecutor {
    executor: Arc<StepFlowExecutor>,
    flow: Arc<Flow>,
    state_store: Arc<dyn StateStore>,
    context: ExecutionContext,
    resolver: ValueResolver,
    tracker: DependencyTracker,
}

impl WorkflowExecutor {
    fn new(
        executor: Arc<StepFlowExecutor>,
        flow: Arc<Flow>,
        execution_id: Uuid,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
        dependencies: Arc<crate::tracker::Dependencies>,
    ) -> Self {
        let resolver = ValueResolver::new(execution_id, input, state_store.clone());
        let context = executor.execution_context(execution_id);
        let tracker = DependencyTracker::new(dependencies);

        Self {
            executor,
            flow,
            state_store,
            context,
            resolver,
            tracker,
        }
    }

    /// Execute the workflow and return the result.
    pub async fn execute(mut self) -> Result<FlowResult> {
        let mut running_tasks = FuturesUnordered::new();

        tracing::debug!("Starting execution of {} steps", self.flow.steps.len());

        // Start initial unblocked steps
        let initial_unblocked = self.tracker.unblocked_steps();
        tracing::debug!(
            "Initially runnable steps: [{}]",
            initial_unblocked
                .iter()
                .map(|idx| self.flow.steps[idx].id.as_str())
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

            // Record the completed result in the state store
            let step_id = &self.flow.steps[completed_step_index].id;
            tracing::debug!(
                "Step {} completed, newly unblocked steps: [{}]",
                step_id,
                newly_unblocked
                    .iter()
                    .map(|idx| self.flow.steps[idx].id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            self.state_store
                .record_step_result(
                    self.context.execution_id(),
                    StepResult::new(completed_step_index, step_id, step_result),
                )
                .await
                .map_err(|_| ExecutionError::RecordResult)?;

            // Start newly unblocked steps
            self.start_unblocked_steps(&newly_unblocked, &mut running_tasks)
                .await?;
        }

        // All tasks completed - try to complete the workflow
        match self.complete_workflow().await {
            Ok(output) => {
                tracing::debug!("Workflow completed successfully");
                Ok(output)
            }
            Err(e) => {
                tracing::warn!(
                    "Workflow execution stalled - unable to resolve final output: {:?}",
                    e
                );
                Err(ExecutionError::Deadlock.into())
            }
        }
    }

    /// Start all newly unblocked steps, handling skips and starting executions.
    async fn start_unblocked_steps(
        &mut self,
        unblocked: &BitSet,
        running_tasks: &mut FuturesUnordered<BoxFuture<'static, (usize, Result<FlowResult>)>>,
    ) -> Result<()> {
        let mut steps_to_process = unblocked.clone();
        while !steps_to_process.is_empty() {
            let mut additional_unblocked = BitSet::new();

            for step_index in steps_to_process.iter() {
                // Extract step data to avoid borrowing issues
                let (step_id, skip_if, step_input) = {
                    let step = &self.flow.steps[step_index];
                    (step.id.clone(), step.skip_if.clone(), step.input.clone())
                };

                // Check skip condition if present
                if let Some(skip_if) = &skip_if {
                    if self.should_skip_step(skip_if).await? {
                        additional_unblocked
                            .union_with(&self.skip_step(&step_id, step_index).await?);
                        continue;
                    }
                }

                // Try to resolve step inputs - if any input is skipped, the step should be skipped
                let step_input = match self.resolver.resolve(&step_input).await {
                    Ok(FlowResult::Success { result }) => result,
                    Ok(FlowResult::Skipped) => {
                        // Step inputs contain skipped values - skip this step
                        additional_unblocked
                            .union_with(&self.skip_step(&step_id, step_index).await?);
                        continue;
                    }
                    Ok(FlowResult::Failed { error }) => {
                        tracing::error!(
                            "Failed to resolve inputs for step {} - input resolution failed: {:?}",
                            step_id,
                            error
                        );
                        return Err(ExecutionError::StepFailed { step: step_id }.into());
                    }
                    Err(e) => {
                        tracing::error!("Failed to resolve inputs for step {}: {:?}", step_id, e);
                        return Err(e);
                    }
                };

                // Start step execution
                self.start_step_execution(step_index, step_input, running_tasks)
                    .await?;
            }

            steps_to_process = additional_unblocked;
        }

        Ok(())
    }

    /// Check if a step should be skipped based on its skip condition.
    async fn should_skip_step(&self, skip_if: &Expr) -> Result<bool> {
        // Resolve the skip condition expression using the current state
        let resolved_value = self.resolver.resolve_expr(skip_if).await?;

        match resolved_value {
            FlowResult::Success { result } => {
                // Check if the resolved value is truthy
                Ok(result.is_truthy())
            }
            FlowResult::Skipped => {
                // If the skip condition itself was skipped, default to not skipping
                // (skip condition being skipped shouldn't cause the step to be skipped)
                Ok(false)
            }
            FlowResult::Failed { .. } => {
                // If skip condition failed, default to not skipping
                Ok(false)
            }
        }
    }

    /// Skip a step and record the result.
    async fn skip_step(&mut self, step_id: &str, step_index: usize) -> Result<BitSet> {
        tracing::debug!("Skipping step {} at index {}", step_id, step_index);

        let newly_unblocked_from_skip = self.tracker.complete_step(step_index);
        let skip_result = FlowResult::Skipped;

        // Record the skipped result in the state store
        self.state_store
            .record_step_result(
                self.context.execution_id(),
                StepResult::new(step_index, step_id, skip_result),
            )
            .await
            .map_err(|_| ExecutionError::RecordResult)?;

        tracing::debug!(
            "Step {} skipped, newly unblocked steps: [{}]",
            step_id,
            newly_unblocked_from_skip
                .iter()
                .map(|idx| self.flow.steps[idx].id.as_str())
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
        let step = &self.flow.steps[step_index];
        tracing::debug!("Starting execution of step {}", step.id);

        // Get plugin for this step
        let plugin = self.executor.get_plugin(&step.component).await?;

        // Clone necessary data for the async task
        let flow = self.flow.clone();
        let context = self.context.clone();

        // Create the async task
        let task_future: BoxFuture<'static, (usize, Result<FlowResult>)> = Box::pin(async move {
            let step = &flow.steps[step_index];
            let result = execute_step_async(plugin, step, step_input, context).await;
            (step_index, result)
        });

        running_tasks.push(task_future);

        Ok(())
    }

    /// Resolve the workflow output.
    async fn complete_workflow(&self) -> Result<FlowResult> {
        // Resolve the workflow output expression
        let output_template = ValueRef::new(self.flow.output.clone());
        self.resolver.resolve(&output_template).await
    }
}

/// Execute a single step asynchronously.
async fn execute_step_async(
    plugin: Arc<DynPlugin<'static>>,
    step: &stepflow_core::workflow::Step,
    input: ValueRef,
    context: ExecutionContext,
) -> Result<FlowResult> {
    // Execute the component
    let result = plugin
        .execute(&step.component, context, input)
        .await
        .map_err(|_e| error_stack::report!(ExecutionError::PluginError))?;

    // Apply error handling logic if the step failed
    let final_result = match result {
        FlowResult::Failed { error } => {
            match &step.on_error {
                stepflow_core::workflow::ErrorAction::Skip => {
                    tracing::debug!("Step {} failed with error {:?}, skipping", step.id, error);
                    FlowResult::Skipped
                }
                stepflow_core::workflow::ErrorAction::UseDefault { default_value } => {
                    tracing::debug!("Step {} failed, using default value", step.id);
                    let value = default_value
                        .as_ref()
                        .map(|v| v.as_ref().clone())
                        .unwrap_or(serde_json::Value::Null);
                    FlowResult::Success {
                        result: ValueRef::new(value),
                    }
                }
                stepflow_core::workflow::ErrorAction::Fail => {
                    tracing::error!("Step {} failed, failing workflow", step.id);
                    return Err(error_stack::report!(ExecutionError::StepFailed {
                        step: step.id.clone(),
                    })
                    .attach_printable(error));
                }
                stepflow_core::workflow::ErrorAction::Retry => {
                    // TODO: Implement retry logic
                    tracing::warn!("Retry not implemented for step {}, failing", step.id);
                    return Err(error_stack::report!(ExecutionError::StepFailed {
                        step: step.id.clone(),
                    })
                    .attach_printable(error));
                }
            }
        }
        other => other,
    };

    Ok(final_result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_mock::{MockComponentBehavior, MockPlugin};
    use stepflow_state::InMemoryStateStore;

    #[tokio::test]
    async fn test_dependency_tracking_basic() {
        let workflow_yaml = r#"
            steps:
              - id: step1
                component: mock://test
                input:
                  value: 42
              - id: step2
                component: mock://test
                input:
                  $from:
                    step: step1
            output:
              $from:
                step: step2
        "#;

        let flow: Flow = serde_yaml_ng::from_str(workflow_yaml).unwrap();

        // Build dependencies
        let dependencies = build_dependencies_from_flow(&flow).unwrap();
        let tracker = DependencyTracker::new(dependencies);

        // Only step1 should be runnable initially
        let unblocked = tracker.unblocked_steps();
        assert_eq!(unblocked.len(), 1);
        assert!(unblocked.contains(0)); // step1

        // This confirms the tracker integration is working
    }

    #[tokio::test]
    async fn test_simple_workflow_execution() {
        let workflow_yaml = r#"
            steps:
              - id: step1
                component: mock://simple
                input:
                  $from:
                    workflow: input
            output:
              $from:
                step: step1
        "#;

        let workflow: Flow = serde_yaml_ng::from_str(workflow_yaml).unwrap();
        let input = ValueRef::new(json!({"message": "hello"}));

        // Set up mock plugin
        let mut mock_plugin = MockPlugin::new();
        mock_plugin.mock_component("mock://simple").behavior(
            ValueRef::new(json!({"message": "hello"})),
            MockComponentBehavior::result(FlowResult::Success {
                result: ValueRef::new(json!({"output": "processed"})),
            }),
        );

        let executor = crate::executor::StepFlowExecutor::new_in_memory();
        let dyn_plugin = stepflow_plugin::DynPlugin::boxed(mock_plugin);
        executor
            .register_plugin("mock".to_string(), dyn_plugin)
            .await
            .unwrap();

        let result = execute_workflow(
            executor,
            Arc::new(workflow),
            Uuid::new_v4(),
            input,
            Arc::new(InMemoryStateStore::new()),
        )
        .await
        .unwrap();

        match result {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!({"output": "processed"}));
            }
            _ => panic!("Expected successful result, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_step_dependencies() {
        let workflow_yaml = r#"
            steps:
              - id: step1
                component: mock://first
                input:
                  $from:
                    workflow: input
              - id: step2
                component: mock://second
                input:
                  $from:
                    step: step1
            output:
              $from:
                step: step2
        "#;

        let workflow: Flow = serde_yaml_ng::from_str(workflow_yaml).unwrap();
        let input = ValueRef::new(json!({"value": 10}));

        // Set up mock plugin
        let mut mock_plugin = MockPlugin::new();
        mock_plugin.mock_component("mock://first").behavior(
            ValueRef::new(json!({"value": 10})),
            MockComponentBehavior::result(FlowResult::Success {
                result: ValueRef::new(json!({"result": 20})),
            }),
        );
        mock_plugin.mock_component("mock://second").behavior(
            ValueRef::new(json!({"result": 20})),
            MockComponentBehavior::result(FlowResult::Success {
                result: ValueRef::new(json!({"final": 30})),
            }),
        );

        let executor = crate::executor::StepFlowExecutor::new_in_memory();
        let dyn_plugin = stepflow_plugin::DynPlugin::boxed(mock_plugin);
        executor
            .register_plugin("mock".to_string(), dyn_plugin)
            .await
            .unwrap();

        let result = execute_workflow(
            executor,
            Arc::new(workflow),
            Uuid::new_v4(),
            input,
            Arc::new(InMemoryStateStore::new()),
        )
        .await
        .unwrap();

        match result {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!({"final": 30}));
            }
            _ => panic!("Expected successful result, got: {:?}", result),
        }
    }
}
