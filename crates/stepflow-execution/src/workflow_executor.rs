use std::sync::Arc;

use bit_set::BitSet;
use error_stack::ResultExt as _;
use futures::{StreamExt as _, future::BoxFuture, stream::FuturesUnordered};
use stepflow_core::{
    FlowError, FlowResult,
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
    let workflow_executor =
        WorkflowExecutor::new(executor, flow, execution_id, input, state_store)?;

    workflow_executor.execute_to_completion().await
}

/// Workflow executor that manages the execution of a single workflow.
///
/// This serves as the core execution engine that can be used directly for
/// run-to-completion execution, or controlled step-by-step by the debug session.
pub struct WorkflowExecutor {
    /// Dependency tracker for determining runnable steps
    tracker: DependencyTracker,
    /// Value resolver for resolving step inputs
    resolver: ValueResolver,
    /// State store for step results
    state_store: Arc<dyn StateStore>,
    /// Executor for getting plugins and execution context
    executor: Arc<StepFlowExecutor>,
    /// The workflow being executed
    flow: Arc<Flow>,
    /// Execution context for this session
    context: ExecutionContext,
}

impl WorkflowExecutor {
    /// Create a new workflow executor for the given workflow and input.
    pub fn new(
        executor: Arc<StepFlowExecutor>,
        flow: Arc<Flow>,
        execution_id: Uuid,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
    ) -> Result<Self> {
        // Build dependencies for the workflow
        let dependencies = build_dependencies_from_flow(&flow)?;
        let tracker = DependencyTracker::new(dependencies);

        // Create value resolver
        let resolver = ValueResolver::new(execution_id, input, state_store.clone());

        // Create execution context
        let context = executor.execution_context(execution_id);

        Ok(Self {
            tracker,
            resolver,
            state_store,
            executor,
            flow,
            context,
        })
    }

    /// Get the execution ID for this executor.
    pub fn execution_id(&self) -> Uuid {
        self.context.execution_id()
    }

    /// Get a reference to the flow being executed.
    pub fn flow(&self) -> &Arc<Flow> {
        &self.flow
    }

    /// Get currently runnable step indices.
    pub fn get_runnable_step_indices(&self) -> BitSet {
        self.tracker.unblocked_steps()
    }

    /// Execute the workflow to completion using parallel execution.
    pub async fn execute_to_completion(mut self) -> Result<FlowResult> {
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
                .change_context_lazy(|| ExecutionError::RecordResult(step_id.clone()))?;

            // Start newly unblocked steps
            self.start_unblocked_steps(&newly_unblocked, &mut running_tasks)
                .await?;
        }

        // All tasks completed - try to complete the workflow
        self.resolve_workflow_output().await
    }

    /// Execute a single step by index and record the result.
    pub async fn execute_step_by_index(&mut self, step_index: usize) -> Result<FlowResult> {
        let step = &self.flow.steps[step_index];

        // Check skip condition if present
        if let Some(skip_if) = &step.skip_if {
            if self.should_skip_step(skip_if).await? {
                let result = FlowResult::Skipped;
                self.record_step_completion(step_index, &result).await?;
                return Ok(result);
            }
        }

        // Resolve step inputs
        let step_input = match self.resolver.resolve(&step.input).await {
            Ok(FlowResult::Success { result }) => result,
            Ok(FlowResult::Skipped) => {
                // Step inputs contain skipped values - skip this step
                let result = FlowResult::Skipped;
                self.record_step_completion(step_index, &result).await?;
                return Ok(result);
            }
            Ok(FlowResult::Failed { error: _ }) => {
                return Err(ExecutionError::StepFailed {
                    step: step.id.clone(),
                }
                .into());
            }
            Err(e) => return Err(e),
        };

        // Get plugin and execute the step
        let plugin = self.executor.get_plugin(&step.component).await?;
        let result = execute_step_async(plugin, step, step_input, self.context.clone()).await?;

        // Record the result
        self.record_step_completion(step_index, &result).await?;

        Ok(result)
    }

    /// Record step completion and update tracker.
    pub async fn record_step_completion(
        &mut self,
        step_index: usize,
        result: &FlowResult,
    ) -> Result<()> {
        // Update dependency tracker
        self.tracker.complete_step(step_index);

        // Record in state store
        let step_id = &self.flow.steps[step_index].id;
        self.state_store
            .record_step_result(
                self.context.execution_id(),
                StepResult::new(step_index, step_id, result.clone()),
            )
            .await
            .change_context_lazy(|| ExecutionError::RecordResult(step_id.clone()))?;

        Ok(())
    }

    /// Resolve the workflow output.
    pub async fn resolve_workflow_output(&self) -> Result<FlowResult> {
        let output_template = ValueRef::new(self.flow.output.clone());
        self.resolver.resolve(&output_template).await
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
        let resolved_value = self.resolver.resolve_expr(skip_if).await?;

        match resolved_value {
            FlowResult::Success { result } => Ok(result.is_truthy()),
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
                    let step = &self.flow.steps[step_index];
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
                let step_input = match self.resolver.resolve(&step_input).await {
                    Ok(FlowResult::Success { result }) => result,
                    Ok(FlowResult::Skipped) => {
                        // Step inputs contain skipped values - propagate the skip
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

        // Record the skipped result in the state store
        self.state_store
            .record_step_result(
                self.context.execution_id(),
                StepResult::new(step_index, step_id, skip_result),
            )
            .await
            .change_context_lazy(|| ExecutionError::RecordResult(step_id.to_owned()))?;

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
}

/// Execute a single step asynchronously.
pub(crate) async fn execute_step_async(
    plugin: Arc<DynPlugin<'static>>,
    step: &stepflow_core::workflow::Step,
    input: ValueRef,
    context: ExecutionContext,
) -> Result<FlowResult> {
    // Execute the component
    let result = plugin.execute(&step.component, context, input).await;

    let result = match result {
        Err(e) => {
            tracing::warn!("Error executing step {}: {:?}", step.id, e);
            FlowResult::Failed {
                error: FlowError {
                    message: e.to_string().into(),
                    // TODO: Better error codes.
                    code: 0,
                    data: None,
                },
            }
        }
        Ok(result) => result,
    };

    match &result {
        FlowResult::Failed { error } => {
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
                    let result = match default_value {
                        Some(default) => default.clone(),
                        None => ValueRef::new(serde_json::Value::Null),
                    };
                    Ok(FlowResult::Success { result })
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::workflow::{Component, ErrorAction, Flow, Step};
    use stepflow_mock::{MockComponentBehavior, MockPlugin};
    use stepflow_state::InMemoryStateStore;

    /// Helper function to create workflow from YAML string with simple mock behaviors
    /// Each component gets a single behavior that will be used regardless of input
    pub async fn create_workflow_from_yaml_simple(
        yaml_str: &str,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> (Arc<crate::executor::StepFlowExecutor>, Arc<Flow>) {
        // Parse the YAML workflow
        let flow: Flow = serde_yaml_ng::from_str(yaml_str).expect("Failed to parse YAML workflow");
        let flow = Arc::new(flow);

        // Create executor with mock plugin
        let executor = crate::executor::StepFlowExecutor::new_in_memory();
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
        executor
            .register_plugin("mock".to_string(), dyn_plugin)
            .await
            .unwrap();

        (executor, flow)
    }

    /// Execute a workflow from YAML string with given input
    pub async fn execute_workflow_from_yaml_simple(
        yaml_str: &str,
        input: serde_json::Value,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> Result<FlowResult> {
        let (executor, flow) = create_workflow_from_yaml_simple(yaml_str, mock_behaviors).await;
        let execution_id = Uuid::new_v4();
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let input_ref = ValueRef::new(input);

        execute_workflow(executor, flow, execution_id, input_ref, state_store).await
    }

    /// Create a WorkflowExecutor from YAML string for step-by-step testing
    pub async fn create_workflow_executor_from_yaml_simple(
        yaml_str: &str,
        input: serde_json::Value,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> Result<WorkflowExecutor> {
        let (executor, flow) = create_workflow_from_yaml_simple(yaml_str, mock_behaviors).await;
        let execution_id = Uuid::new_v4();
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let input_ref = ValueRef::new(input);

        WorkflowExecutor::new(executor, flow, execution_id, input_ref, state_store)
    }

    fn create_test_step(id: &str, input: serde_json::Value) -> Step {
        Step {
            id: id.to_string(),
            component: Component::parse("mock://test").unwrap(),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::Fail,
            input: ValueRef::new(input),
        }
    }

    fn create_test_flow(steps: Vec<Step>, output: serde_json::Value) -> Flow {
        Flow {
            name: None,
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps,
            output,
            test: None,
        }
    }

    #[tokio::test]
    async fn test_dependency_tracking_basic() {
        // Test that we can create dependencies and tracker correctly
        let steps = vec![
            create_test_step("step1", json!({"value": 42})),
            create_test_step("step2", json!({"$from": {"step": "step1"}})),
        ];

        let flow = create_test_flow(steps, json!({"$from": {"step": "step2"}}));

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

        let input_value = json!({"message": "hello"});
        let mock_behaviors = vec![(
            "mock://simple",
            FlowResult::Success {
                result: ValueRef::new(json!({"output": "processed"})),
            },
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, input_value, mock_behaviors)
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

        let workflow_input = json!({"value": 10});
        let step1_output = json!({"result": 20});

        let mock_behaviors = vec![
            (
                "mock://first",
                FlowResult::Success {
                    result: ValueRef::new(step1_output.clone()),
                },
            ),
            (
                "mock://second",
                FlowResult::Success {
                    result: ValueRef::new(json!({"final": 30})),
                },
            ),
        ];

        let result =
            execute_workflow_from_yaml_simple(workflow_yaml, workflow_input, mock_behaviors)
                .await
                .unwrap();

        match result {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!({"final": 30}));
            }
            _ => panic!("Expected successful result, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_workflow_executor_step_by_step() {
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

        let workflow_input = json!({"value": 10});
        let mock_behaviors = vec![
            (
                "mock://first",
                FlowResult::Success {
                    result: ValueRef::new(json!({"result": 20})),
                },
            ),
            (
                "mock://second",
                FlowResult::Success {
                    result: ValueRef::new(json!({"final": 30})),
                },
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
        match result {
            FlowResult::Success { result } => {
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
        match result {
            FlowResult::Success { result } => {
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
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!({"final": 30}));
            }
            _ => panic!("Expected successful final result"),
        }
    }

    #[tokio::test]
    async fn test_workflow_executor_parallel_vs_sequential() {
        let workflow_yaml = r#"
steps:
  - id: step1
    component: mock://parallel1
    input:
      $from:
        workflow: input
  - id: step2
    component: mock://parallel2
    input:
      $from:
        workflow: input
  - id: final
    component: mock://combiner
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
                "mock://parallel1",
                FlowResult::Success {
                    result: ValueRef::new(step1_output.clone()),
                },
            ),
            (
                "mock://parallel2",
                FlowResult::Success {
                    result: ValueRef::new(step2_output.clone()),
                },
            ),
            (
                "mock://combiner",
                FlowResult::Success {
                    result: ValueRef::new(final_output.clone()),
                },
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
                "mock://parallel1",
                FlowResult::Success {
                    result: ValueRef::new(step1_output.clone()),
                },
            ),
            (
                "mock://parallel2",
                FlowResult::Success {
                    result: ValueRef::new(step2_output.clone()),
                },
            ),
            (
                "mock://combiner",
                FlowResult::Success {
                    result: ValueRef::new(final_output.clone()),
                },
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
            (FlowResult::Success { result: p }, FlowResult::Success { result: s }) => {
                assert_eq!(p.as_ref(), s.as_ref());
                assert_eq!(p.as_ref(), &final_output);
            }
            _ => panic!("Both executions should be successful"),
        }
    }

    #[tokio::test]
    async fn test_error_handling_skip() {
        let workflow_yaml = r#"
steps:
  - id: failing_step
    component: mock://error
    on_error:
      action: skip
    input:
      mode: error
output:
  $from:
    step: failing_step
"#;

        let mock_behaviors = vec![(
            "mock://error",
            FlowResult::Failed {
                error: stepflow_core::FlowError::new(500, "Test error"),
            },
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        // The workflow should complete with skipped result
        match result {
            FlowResult::Skipped => {
                // Expected - the step failed but was configured to skip
            }
            _ => panic!("Expected skipped result, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_error_handling_use_default_with_value() {
        let workflow_yaml = r#"
steps:
  - id: failing_step
    component: mock://error
    on_error:
      action: use_default
      default_value: {"fallback": "value"}
    input:
      mode: error
output:
  $from:
    step: failing_step
"#;

        let mock_behaviors = vec![(
            "mock://error",
            FlowResult::Failed {
                error: stepflow_core::FlowError::new(500, "Test error"),
            },
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        match result {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!({"fallback": "value"}));
            }
            _ => panic!("Expected success with default value, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_error_handling_use_default_without_value() {
        let workflow_yaml = r#"
steps:
  - id: failing_step
    component: mock://error
    on_error:
      action: use_default
    input:
      mode: error
output:
  $from:
    step: failing_step
"#;

        let mock_behaviors = vec![(
            "mock://error",
            FlowResult::Failed {
                error: stepflow_core::FlowError::new(500, "Test error"),
            },
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        match result {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &serde_json::Value::Null);
            }
            _ => panic!("Expected success with null value, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_error_handling_fail() {
        let workflow_yaml = r#"
steps:
  - id: failing_step
    component: mock://error
    on_error:
      action: fail
    input:
      mode: error
output:
  $from:
    step: failing_step
"#;

        let mock_behaviors = vec![(
            "mock://error",
            FlowResult::Failed {
                error: stepflow_core::FlowError::new(500, "Test error"),
            },
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        match result {
            FlowResult::Failed { error } => {
                assert_eq!(error.code, 500);
                assert_eq!(error.message, "Test error");
            }
            _ => panic!("Expected failed result, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_error_handling_success_case() {
        let workflow_yaml = r#"
steps:
  - id: success_step
    component: mock://success
    on_error:
      action: skip
    input: {}
output:
  $from:
    step: success_step
"#;

        let mock_behaviors = vec![(
            "mock://success",
            FlowResult::Success {
                result: ValueRef::new(json!({"result": "success"})),
            },
        )];

        let result = execute_workflow_from_yaml_simple(workflow_yaml, json!({}), mock_behaviors)
            .await
            .unwrap();

        match result {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!({"result": "success"}));
            }
            _ => panic!("Expected success result, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_error_handling_skip_with_multi_step() {
        // Test that when a step is skipped, downstream steps handle it correctly
        let workflow_yaml = r#"
steps:
  - id: failing_step
    component: mock://error
    on_error:
      action: skip
    input:
      mode: error
  - id: downstream_step
    component: mock://success
    input:
      $from:
        step: failing_step
output:
  $from:
    step: downstream_step
"#;

        let mock_behaviors = vec![
            (
                "mock://error",
                FlowResult::Failed {
                    error: stepflow_core::FlowError::new(500, "Test error"),
                },
            ),
            (
                "mock://success",
                FlowResult::Success {
                    result: ValueRef::new(json!({"handled_skip": true})),
                },
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
            _ => panic!(
                "Expected skipped result for downstream step, got: {:?}",
                result
            ),
        }
    }
}
