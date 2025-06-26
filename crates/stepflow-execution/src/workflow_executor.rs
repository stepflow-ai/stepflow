use std::sync::Arc;

use bit_set::BitSet;
use error_stack::ResultExt as _;
use futures::{StreamExt as _, future::BoxFuture, stream::FuturesUnordered};
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowResult,
    status::{StepExecution, StepStatus as CoreStepStatus},
    workflow::{Expr, Flow, ValueRef},
};
use stepflow_plugin::{DynPlugin, ExecutionContext, Plugin as _};
use stepflow_state::{StateStore, StepResult};
use uuid::Uuid;
use tokio::sync::mpsc;

use crate::{ExecutionError, Result, StepFlowExecutor, value_resolver::ValueResolver};

/// Helper macro for streaming step logging
macro_rules! stream_log {
    ($level:ident, $step_id:expr, $($arg:tt)*) => {
        tracing::$level!("[STREAM {}] {}", $step_id, format!($($arg)*))
    };
}

/// Execute a workflow and return the result.
pub(crate) async fn execute_workflow(
    executor: Arc<StepFlowExecutor>,
    flow: Arc<Flow>,
    workflow_hash: FlowHash,
    execution_id: Uuid,
    input: ValueRef,
    state_store: Arc<dyn StateStore>,
) -> Result<FlowResult> {
    // Check if there's already a debug session for this execution ID
    let existing_debug_session = executor.get_debug_session(execution_id).await;
    
    if let Some(debug_session) = existing_debug_session {
        // Use the existing debug session
        tracing::info!("Using existing debug session for execution ID: {}", execution_id);
        let mut workflow_executor = debug_session.lock().await;
        workflow_executor.execute_to_completion().await
    } else {
        // Create a new workflow executor
        tracing::info!("Executing workflow using tracker-based execution");
        let mut workflow_executor = WorkflowExecutor::new(
            executor,
            flow,
            workflow_hash,
            execution_id,
            input,
            state_store,
        )?;

        workflow_executor.execute_to_completion().await
    }
}

/// Workflow executor that manages the execution of a single workflow.
///
/// This serves as the core execution engine that can be used directly for
/// run-to-completion execution, or controlled step-by-step by the debug session.
pub struct WorkflowExecutor {
    /// Dependency tracker for determining runnable steps
    tracker: stepflow_analysis::DependencyTracker,
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
    /// Optional streaming pipeline coordinator
    streaming_coordinator: Option<Arc<tokio::sync::Mutex<StreamingPipelineCoordinator>>>,
}

impl WorkflowExecutor {
    /// Create a new workflow executor for the given workflow and input.
    pub fn new(
        executor: Arc<StepFlowExecutor>,
        flow: Arc<Flow>,
        workflow_hash: FlowHash,
        execution_id: Uuid,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
    ) -> Result<Self> {
        // Build dependencies for the workflow using the analysis crate
        let analysis =
            stepflow_analysis::analyze_workflow_dependencies(flow.clone(), workflow_hash)
                .change_context(ExecutionError::AnalysisError)?;

        // Create tracker from analysis
        let tracker = analysis.new_dependency_tracker();

        // Create value resolver
        let resolver = ValueResolver::new(execution_id, input, state_store.clone());

        // Create execution context
        let context = executor.execution_context(execution_id);

        // Initialize streaming coordinator if workflow has streaming steps
        let streaming_coordinator = if flow.steps.iter().any(|step| step.streaming) {
            let mut pipeline_steps: Vec<usize> = flow.steps.iter()
                .enumerate()
                .filter(|(_, step)| step.streaming)
                .map(|(index, _)| index)
                .collect();
            
            if !pipeline_steps.is_empty() {
                // Log the initial order (source order)
                tracing::info!("Initial pipeline order (source order): {:?}",
                    pipeline_steps.iter().map(|i| &flow.steps[*i].id).collect::<Vec<_>>()
                );
                
                // Sort pipeline steps by dependencies using a topological sort
                pipeline_steps = sort_streaming_steps_by_dependencies(&flow, pipeline_steps)?;
                
                // Log the final pipeline order to verify it's correct
                tracing::info!("Final pipeline will run in this order: {:?}",
                    pipeline_steps.iter().map(|i| &flow.steps[*i].id).collect::<Vec<_>>()
                );
                tracing::info!("Pipeline step indices and components: {:?}",
                    pipeline_steps.iter().map(|i| (*i, &flow.steps[*i].id, &flow.steps[*i].component)).collect::<Vec<_>>()
                );
                
                tracing::info!("[DEBUG-INIT] Creating streaming coordinator in WorkflowExecutor::new");
                let coordinator = StreamingPipelineCoordinator::new(
                    executor.clone(),
                    flow.clone(),
                    pipeline_steps,
                    context.clone(),
                    resolver.clone(),
                );
                Some(Arc::new(tokio::sync::Mutex::new(coordinator)))
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            tracker,
            resolver,
            state_store,
            executor,
            flow,
            context,
            streaming_coordinator,
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

    /// Check if the streaming pipeline is still active (has active receivers)
    pub fn is_streaming_pipeline_active(&self) -> bool {
        if let Some(coord_arc) = &self.streaming_coordinator {
            // For now, just check if coordinator exists - we can't easily check receivers without async
            true
        } else {
            // No coordinator means no streaming pipeline
            false
        }
    }

    /// Get currently runnable step indices.
    pub fn get_runnable_step_indices(&self) -> BitSet {
        self.tracker.unblocked_steps()
    }

    /// Execute the workflow to completion using parallel execution.
    /// This method runs until all steps are completed and returns the final result.
    pub async fn execute_to_completion(&mut self) -> Result<FlowResult> {
        let mut running_tasks = FuturesUnordered::new();

        tracing::debug!("Starting execution of {} steps", self.flow.steps.len());

        // Start streaming pipeline coordinator concurrently if it exists
        let streaming_task = if let Some(coordinator_arc) = &self.streaming_coordinator {
            tracing::info!("Starting streaming pipeline coordinator concurrently with main execution");
            
            let coord = coordinator_arc.clone();
            // Start the pipeline execution in a separate task (single-phase, no setup needed)
            Some(tokio::spawn(async move {
                StreamingPipelineCoordinator::run_pipeline_without_lock(coord).await
            }))
        } else {
            None
        };

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

            // Update step status based on result
            let final_status = match &step_result {
                FlowResult::Success { .. } => stepflow_core::status::StepStatus::Completed,
                FlowResult::Failed { .. } => stepflow_core::status::StepStatus::Failed,
                FlowResult::Skipped => stepflow_core::status::StepStatus::Skipped,
                FlowResult::Streaming { .. } => stepflow_core::status::StepStatus::Running, // Keep as running for streaming
            };

            self.state_store
                .update_step_status(
                    self.context.execution_id(),
                    completed_step_index,
                    final_status,
                )
                .await
                .change_context_lazy(|| ExecutionError::StateError)?;

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

        // Wait for streaming pipeline to complete if it was started
        if let Some(streaming_task) = streaming_task {
            tracing::info!("Waiting for streaming pipeline coordinator to complete");
            match streaming_task.await {
                Ok(result) => {
                    if let Err(e) = result {
                        tracing::warn!("Streaming pipeline coordinator completed with error: {:?}", e);
                    } else {
                        tracing::info!("Streaming pipeline coordinator completed successfully");
                    }
                }
                Err(e) => {
                    tracing::warn!("Streaming pipeline coordinator task panicked: {:?}", e);
                }
            }
        }

        self.resolve_workflow_output().await
    }

    /// List all steps in the workflow with their current status.
    pub async fn list_all_steps(&self) -> Vec<StepExecution> {
        // Get step info from persistent storage (single query for all steps)
        let step_infos = self
            .state_store
            .get_step_info_for_execution(self.context.execution_id())
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
        for (idx, step) in self.flow.steps.iter().enumerate() {
            let state = status_map.get(&idx).copied().unwrap_or_else(|| {
                // Fallback: check if step is runnable using in-memory tracker
                let runnable = self.tracker.unblocked_steps();
                if runnable.contains(idx) {
                    CoreStepStatus::Runnable
                } else {
                    // Default to blocked for steps without persistent status
                    CoreStepStatus::Blocked
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
            .get_runnable_steps(self.context.execution_id())
            .await
            .unwrap_or_default();

        // Convert step infos to step executions using cached workflow data
        runnable_step_infos
            .iter()
            .map(|step_info| {
                let step = &self.flow.steps[step_info.step_index];
                StepExecution::new(
                    step_info.step_index,
                    step.id.clone(),
                    step.component.to_string(),
                    CoreStepStatus::Runnable,
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
            .steps
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
            .steps
            .iter()
            .position(|step| step.id == target_step_id)
            .ok_or_else(|| ExecutionError::StepNotFound {
                step: target_step_id.to_string(),
            })?;

        // Keep executing until the target step is runnable or completed
        let max_iterations = 1000; // Safety limit to prevent infinite loops
        let mut iteration_count = 0;
        
        loop {
            iteration_count += 1;
            if iteration_count > max_iterations {
                tracing::error!("execute_until_runnable exceeded maximum iterations ({}), stopping execution", max_iterations);
                return Err(ExecutionError::StepFailed { 
                    step: format!("execute_until_runnable for {}", target_step_id) 
                }.into());
            }
            
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
            .list_step_results(self.context.execution_id())
            .await
            .change_context(ExecutionError::StateError)?;

        let completed_steps = step_results
            .into_iter()
            .map(|step_result| StepExecutionResult {
                metadata: StepMetadata {
                    step_index: step_result.step_idx(),
                    step_id: step_result.step_id().to_string(),
                    component: self.flow.steps[step_result.step_idx()]
                        .component
                        .to_string(),
                },
                result: step_result.into_result(),
            })
            .collect();

        Ok(completed_steps)
    }

    /// Get the output/result of a specific step by ID.
    pub async fn get_step_output(&self, step_id: &str) -> Result<FlowResult> {
        self.state_store
            .get_step_result_by_id(self.context.execution_id(), step_id)
            .await
            .attach_printable_lazy(|| format!("Failed to get output for step '{}'", step_id))
            .change_context(ExecutionError::StepNotFound {
                step: step_id.to_string(),
            })
    }

    /// Get the details of a specific step for inspection.
    pub async fn inspect_step(&self, step_id: &str) -> Result<StepInspection> {
        let step_index = self
            .flow
            .steps
            .iter()
            .position(|step| step.id == step_id)
            .ok_or_else(|| ExecutionError::StepNotFound {
                step: step_id.to_string(),
            })?;

        let step = &self.flow.steps[step_index];
        let runnable = self.tracker.unblocked_steps();

        let state = if runnable.contains(step_index) {
            CoreStepStatus::Runnable
        } else {
            // Check if step is completed by querying state store
            match self
                .state_store
                .get_step_result_by_index(self.context.execution_id(), step_index)
                .await
            {
                Ok(result) => match result {
                    FlowResult::Success { .. } => CoreStepStatus::Completed,
                    FlowResult::Skipped => CoreStepStatus::Skipped,
                    FlowResult::Failed { .. } => CoreStepStatus::Failed,
                    FlowResult::Streaming { .. } => CoreStepStatus::Running, // Streaming steps are considered running
                },
                Err(_) => CoreStepStatus::Blocked,
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
        let step = &self.flow.steps[step_index];
        let step_id = step.id.clone();
        let component_string = step.component.to_string();

        // Check if the step is runnable
        if !self.tracker.unblocked_steps().contains(step_index) {
            return Err(ExecutionError::StepNotRunnable {
                step: step.id.clone(),
            }
            .into());
        }

        // Update step status to Running
        self.state_store
            .update_step_status(
                self.context.execution_id(),
                step_index,
                stepflow_core::status::StepStatus::Running,
            )
            .await
            .change_context_lazy(|| ExecutionError::StateError)?;

        // Check skip condition if present
        if let Some(skip_if) = &step.skip_if {
            if self.should_skip_step(skip_if).await? {
                let result = FlowResult::Skipped;
                
                // Update step status to Skipped
                self.state_store
                    .update_step_status(
                        self.context.execution_id(),
                        step_index,
                        stepflow_core::status::StepStatus::Skipped,
                    )
                    .await
                    .change_context_lazy(|| ExecutionError::StateError)?;
                
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
        let step_input = match self.resolver.resolve(&step.input).await? {
            FlowResult::Success { result } => result,
            FlowResult::Streaming { stream_id, metadata, chunk, chunk_index, is_final } => {
                // For streaming steps, we can handle streaming inputs
                // For now, just return the metadata as the input
                metadata
            }
            FlowResult::Skipped => {
                return Err(ExecutionError::StepNotRunnable {
                    step: step_id.clone(),
                }
                .into());
            }
            FlowResult::Failed { error } => {
                return Err(ExecutionError::StepFailed { step: step_id }.into());
            }
        };

        // Check if this is a streaming step
        if step.streaming {
            tracing::info!("Step {} is a streaming step, using streaming execution", step.id);
            // For streaming steps, return a StepExecutionResult with a placeholder Streaming result
            let streaming_result = FlowResult::Streaming {
                stream_id: format!("stream_{}", step.id),
                metadata: stepflow_core::workflow::ValueRef::new(serde_json::json!({
                    "step_id": step.id,
                    "step_index": step_index,
                    "streaming": true
                })),
                chunk: "".to_string(),
                chunk_index: 0,
                is_final: false,
            };
            return Ok(StepExecutionResult::new(
                step_index,
                step_id,
                component_string,
                streaming_result,
            ));
        }

        // Regular non-streaming step execution
        let plugin = self.executor.get_plugin(&step.component).await?;
        let flow = self.flow.clone();
        let context = self.context.clone()
            .with_step(self.flow.steps[step_index].id.clone());
        let step = &flow.steps[step_index];
        let result = execute_step_async(plugin, step, step_input, context).await?;
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
        self.resolver.resolve(&self.flow.output).await
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
            FlowResult::Streaming { .. } => Ok(false), // Don't skip if condition references streaming values
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
                    Ok(FlowResult::Streaming { .. }) => {
                        // Step inputs contain streaming values - this is not supported for regular steps
                        tracing::error!(
                            "Step {} has streaming inputs which is not supported for non-streaming steps",
                            step_id
                        );
                        return Err(ExecutionError::StepFailed { step: step_id }.into());
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

        // Update step status to Skipped
        self.state_store
            .update_step_status(
                self.context.execution_id(),
                step_index,
                stepflow_core::status::StepStatus::Skipped,
            )
            .await
            .change_context_lazy(|| ExecutionError::StateError)?;

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

        // Update step status to Running
        self.state_store
            .update_step_status(
                self.context.execution_id(),
                step_index,
                stepflow_core::status::StepStatus::Running,
            )
            .await
            .change_context_lazy(|| ExecutionError::StateError)?;

        // Check if this is a streaming step
        if step.streaming {
            tracing::info!("Step {} is a streaming step, using streaming execution", step.id);
            // For streaming steps, just mark as running; the coordinator will handle execution
            return Ok(());
        }

        // Regular non-streaming step execution
        let plugin = self.executor.get_plugin(&step.component).await?;

        // Clone necessary data for the async task
        let flow = self.flow.clone();
        let context = self.context.clone()
            .with_step(self.flow.steps[step_index].id.clone());

        // Create the async task
        let task_future: BoxFuture<'static, (usize, Result<FlowResult>)> = Box::pin(async move {
            let step = &flow.steps[step_index];
            let result = execute_step_async(plugin.clone(), step, step_input, context).await;
            (step_index, result)
        });

        running_tasks.push(task_future);

        Ok(())
    }

    /// Execute a streaming step continuously.
    /// This method runs the step in a loop, processing chunks as they arrive.
    pub async fn execute_streaming_step(
        &mut self,
        step_index: usize,
    ) -> Result<()> {
        let step = &self.flow.steps[step_index];
        let step_id = step.id.clone();

        // Check if the step is runnable
        if !self.tracker.unblocked_steps().contains(step_index) {
            return Err(ExecutionError::StepNotRunnable {
                step: step.id.clone(),
            }
            .into());
        }

        // Check if this is actually a streaming step
        if !step.streaming {
            return Err(ExecutionError::StepNotRunnable {
                step: step.id.clone(),
            }
            .into());
        }

        // Check if this is part of a streaming pipeline
        if self.is_streaming_pipeline_step(step_index) {
            return self.execute_streaming_pipeline_step(step_index).await;
        }

        // Fallback to individual streaming step execution
        self.execute_individual_streaming_step(step_index).await
    }

    /// Check if a step is part of a streaming pipeline (has streaming inputs/outputs)
    fn is_streaming_pipeline_step(&self, step_index: usize) -> bool {
        let step = &self.flow.steps[step_index];
        
        // Check if this step has streaming inputs from other streaming steps
        for (other_index, other_step) in self.flow.steps.iter().enumerate() {
            if other_index != step_index && other_step.streaming {
                // Check if this step references the other streaming step
                if self.step_references_other_step(step, other_step) {
                    return true;
                }
            }
        }
        
        false
    }

    /// Check if a step references another step in its inputs
    fn step_references_other_step(&self, step: &stepflow_core::workflow::Step, other_step: &stepflow_core::workflow::Step) -> bool {
        // Simple check: look for step references in the input
        let input_str = serde_json::to_string(&step.input).unwrap_or_default();
        input_str.contains(&format!("step: {}", other_step.id))
    }

    /// Execute a step that's part of a streaming pipeline
    async fn execute_streaming_pipeline_step(&mut self, step_index: usize) -> Result<()> {
        let step = &self.flow.steps[step_index];
        let step_id = step.id.clone();

        tracing::info!("Executing streaming pipeline step: {}", step_id);

        // Update step status to Running
        self.state_store
            .update_step_status(
                self.context.execution_id(),
                step_index,
                stepflow_core::status::StepStatus::Running,
            )
            .await
            .change_context_lazy(|| ExecutionError::StateError)?;

        // Reuse the coordinator created in WorkflowExecutor::new
        let pipeline_result = if let Some(coord_arc) = &self.streaming_coordinator {
            StreamingPipelineCoordinator::run_pipeline_without_lock(coord_arc.clone()).await
        } else {
            return Err(ExecutionError::Internal.into());
        };
        
        match pipeline_result {
            Ok(_) => {
                // Update step status to Completed
                self.state_store
                    .update_step_status(
                        self.context.execution_id(),
                        step_index,
                        stepflow_core::status::StepStatus::Completed,
                    )
                    .await
                    .change_context_lazy(|| ExecutionError::StateError)?;
            }
            Err(e) => {
                // Update step status to Failed
                self.state_store
                    .update_step_status(
                        self.context.execution_id(),
                        step_index,
                        stepflow_core::status::StepStatus::Failed,
                    )
                    .await
                    .change_context_lazy(|| ExecutionError::StateError)?;
                return Err(e);
            }
        }

        // Update dependency tracker
        self.tracker.complete_step(step_index);

        Ok(())
    }

    /// Find all steps that are part of the same streaming pipeline
    fn find_streaming_pipeline_steps(&self, start_step_index: usize) -> Vec<usize> {
        let mut pipeline_steps = vec![start_step_index];
        let mut to_check = vec![start_step_index];
        let mut checked = std::collections::HashSet::new();

        while let Some(step_index) = to_check.pop() {
            if checked.contains(&step_index) {
                continue;
            }
            checked.insert(step_index);

            let step = &self.flow.steps[step_index];
            
            // Find steps that this step depends on (streaming inputs)
            for (other_index, other_step) in self.flow.steps.iter().enumerate() {
                if other_step.streaming && self.step_references_other_step(step, other_step) {
                    if !pipeline_steps.contains(&other_index) {
                        pipeline_steps.push(other_index);
                        to_check.push(other_index);
                    }
                }
            }

            // Find steps that depend on this step (streaming outputs)
            for (other_index, other_step) in self.flow.steps.iter().enumerate() {
                if other_step.streaming && self.step_references_other_step(other_step, step) {
                    if !pipeline_steps.contains(&other_index) {
                        pipeline_steps.push(other_index);
                        to_check.push(other_index);
                    }
                }
            }
        }

        pipeline_steps.sort();
        pipeline_steps
    }

    /// Execute an individual streaming step (fallback)
    async fn execute_individual_streaming_step(&mut self, step_index: usize) -> Result<()> {
        let step = &self.flow.steps[step_index];
        let step_id = step.id.clone();

        // Update step status to Running
        self.state_store
            .update_step_status(
                self.context.execution_id(),
                step_index,
                stepflow_core::status::StepStatus::Running,
            )
            .await
            .change_context_lazy(|| ExecutionError::StateError)?;

        // Resolve step inputs
        let step_input = match self.resolver.resolve(&step.input).await? {
            FlowResult::Success { result } => result,
            FlowResult::Streaming { stream_id, metadata, chunk, chunk_index, is_final } => {
                // For streaming steps, we can handle streaming inputs
                // For now, just return the metadata as the input
                metadata
            }
            FlowResult::Skipped => {
                return Err(ExecutionError::StepNotRunnable {
                    step: step_id.clone(),
                }
                .into());
            }
            FlowResult::Failed { error } => {
                return Err(ExecutionError::StepFailed { step: step_id }.into());
            }
        };

        // Get plugin
        let plugin = self.executor.get_plugin(&step.component).await?;

        // For streaming steps, we need to:
        // 1. Call the step once to start the generator
        // 2. Wait for streaming chunks to come through the notification system
        // 3. Process each chunk as it arrives
        // 4. Stop when the final chunk arrives
        
        tracing::info!("[streaming] Starting streaming step {} with initial call", step_id);
        
        // Initial call to start the generator
        let initial_result = execute_step_async(plugin.clone(), step, step_input.clone(), self.context.clone().with_step(step.id.clone())).await?;
            
        match initial_result {
            FlowResult::Streaming { stream_id, metadata, chunk, chunk_index, is_final } => {
                tracing::info!("[streaming] Step {} started generator, received initial chunk (index={}, is_final={})", step_id, chunk_index, is_final);


                // Process the initial chunk
                    let mut chunk_input_data = step_input.as_ref().clone();
                    if let serde_json::Value::Object(ref mut map) = chunk_input_data {
                        map.insert("chunk".to_string(), serde_json::Value::String(chunk.clone()));
                        map.insert("stream_id".to_string(), serde_json::Value::String(stream_id.clone()));
                        map.insert("chunk_index".to_string(), serde_json::Value::Number(chunk_index.into()));
                        map.insert("is_final".to_string(), serde_json::Value::Bool(is_final));
                        if let Some(metadata_obj) = metadata.as_ref().as_object() {
                            for (key, value) in metadata_obj {
                                map.insert(key.clone(), value.clone());
                            }
                        }
                    }
                    let chunk_input = stepflow_core::workflow::ValueRef::new(chunk_input_data);
                    
                    let chunk_result = match tokio::time::timeout(
                        std::time::Duration::from_secs(5), 
                        execute_step_async(plugin.clone(), step, chunk_input, self.context.clone().with_step(step.id.clone()))
                    ).await {
                        Ok(result) => result?,
                        Err(_) => {
                        tracing::warn!("[streaming] Step {} initial chunk processing timed out", step_id);
                        // Return a default result for timeout case
                        FlowResult::Success {
                            result: stepflow_core::workflow::ValueRef::new(serde_json::json!({
                                "error": "timeout",
                                "message": "Initial chunk processing timed out"
                            }))
                        }
                        }
                    };
                    
                    // If this is the final chunk, we're done
                    if is_final {
                    tracing::info!("[streaming] Step {} completed with final chunk from initial call", step_id);
                } else {
                    // Wait for additional chunks to come through the streaming notification system
                    // The chunks will be routed via route_streaming_chunk method
                    tracing::info!("[streaming] Step {} waiting for additional chunks via streaming notifications", step_id);
                    
                    // For now, we'll wait a reasonable amount of time for chunks to arrive
                    // In a more sophisticated implementation, we'd have a proper notification system
                    let mut chunk_count = 1;
                    let max_wait_time = std::time::Duration::from_secs(30); // Wait up to 30 seconds
                    let start_time = std::time::Instant::now();
                    
                    while start_time.elapsed() < max_wait_time {
                        // Sleep briefly to allow chunks to be processed
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        
                        // Check if we should continue waiting
                        // This is a simplified approach - in practice, we'd have proper notification handling
                        chunk_count += 1;
                        if chunk_count % 100 == 0 {
                            tracing::debug!("[streaming] Step {} still waiting for chunks, elapsed: {:?}", step_id, start_time.elapsed());
                        }
                    }
                    
                    tracing::info!("[streaming] Step {} finished waiting for chunks, elapsed: {:?}", step_id, start_time.elapsed());
                    }
                }
                FlowResult::Success { result } => {
                    // Non-streaming result, treat as final
                    tracing::debug!("Streaming step {} completed with success", step_id);
                }
                FlowResult::Failed { error } => {
                    tracing::error!("Streaming step {} failed: {:?}", step_id, error);
                    
                    // Update step status to Failed
                    self.state_store
                        .update_step_status(
                            self.context.execution_id(),
                            step_index,
                            stepflow_core::status::StepStatus::Failed,
                        )
                        .await
                        .change_context_lazy(|| ExecutionError::StateError)?;
                    
                    return Err(ExecutionError::StepFailed { step: step_id }.into());
                }
                FlowResult::Skipped => {
                    tracing::debug!("Streaming step {} skipped", step_id);
            }
        }

        // Update step status to Completed
        self.state_store
            .update_step_status(
                self.context.execution_id(),
                step_index,
                stepflow_core::status::StepStatus::Completed,
            )
            .await
            .change_context_lazy(|| ExecutionError::StateError)?;

        // Update dependency tracker for streaming step
        self.tracker.complete_step(step_index);

        Ok(())
    }

    /// Route a streaming chunk to the appropriate streaming pipeline
    pub async fn route_streaming_chunk(&mut self, chunk: serde_json::Value) -> Result<()> {
        tracing::info!("ROUTE_CHUNK[exec={} addr={:p}] called", self.execution_id(), self as *const _);
        
        if let Some(coord_arc) = &self.streaming_coordinator {
            // Route chunks directly to the coordinator used by run_pipeline_without_lock
            // This ensures chunks are sent to the same channels that the step tasks are listening on
            StreamingPipelineCoordinator::route_chunk_to_running_pipeline(coord_arc.clone(), chunk).await?;
        } else {
            tracing::warn!("No streaming pipeline active for exec {}", self.execution_id());
        }
        Ok(())
    }
    
    /// Find currently active streaming steps using in-memory workflow information
    /// This avoids depending on state store data that might be cleaned up
    fn find_active_streaming_steps_in_memory(&self) -> Vec<usize> {
        let mut active_steps = Vec::new();
        
        for (step_index, step) in self.flow.steps.iter().enumerate() {
            if step.streaming {
                // For streaming steps, assume they are active if they exist in the coordinator
                if let Some(_coord_arc) = &self.streaming_coordinator {
                    // For now, just assume all streaming steps are active
                    // We can't easily check step_receivers without async
                    active_steps.push(step_index);
                    tracing::debug!("Found active streaming step {} in coordinator", step_index);
                }
            }
        }
        
        tracing::debug!("Found {} active streaming steps: {:?}", active_steps.len(), active_steps);
        active_steps
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
    let result = plugin
        .execute(&step.component, context, input)
        .await
        .change_context(ExecutionError::StepFailed {
            step: step.id.to_owned(),
        })?;

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
    pub input: ValueRef,
    pub skip_if: Option<Expr>,
    pub on_error: stepflow_core::workflow::ErrorAction,
    pub state: CoreStepStatus,
}

/// Coordinates streaming execution between multiple steps in a pipeline
struct StreamingPipelineCoordinator {
    executor: Arc<StepFlowExecutor>,
    flow: Arc<Flow>,
    pipeline_steps: Vec<usize>,
    context: ExecutionContext,
    resolver: ValueResolver,
    step_receivers: std::collections::HashMap<String, mpsc::Receiver<FlowResult>>,
    step_downstream_senders: std::collections::HashMap<String, Vec<mpsc::Sender<FlowResult>>>,
    step_senders: std::collections::HashMap<String, mpsc::Sender<FlowResult>>,
}

impl StreamingPipelineCoordinator {
    /// Route chunks to the running pipeline without requiring mutable access to the coordinator
    /// This allows chunks to be routed while the receivers are moved out for step tasks
    pub async fn route_chunk_to_running_pipeline(
        coord_arc: Arc<tokio::sync::Mutex<StreamingPipelineCoordinator>>,
        chunk_json: serde_json::Value,
    ) -> Result<()> {
        let map = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(chunk_json)
            .map_err(|e| ExecutionError::MalformedReference { message: e.to_string() })?;
        
        // Extract chunk metadata
        let chunk_index = map.get("chunk_index")
                             .and_then(|v| v.as_u64())
                             .unwrap_or(0) as usize;
        
        let is_final = map.get("is_final")
                          .and_then(|v| v.as_bool())
                          .unwrap_or(false);
        
        // Extract the step ID this chunk came FROM
        let source_step_id = if let Some(step_id) = map.get("step_id").and_then(|v| v.as_str()) {
            step_id.to_string()
        } else {
            // Default to first step if no step_id provided
            let coord = coord_arc.lock().await;
            let first_step_idx = coord.pipeline_steps[0];
            let first_step_id = coord.flow.steps[first_step_idx].id.clone();
            tracing::info!("No step_id in chunk, defaulting to first pipeline step: {}", first_step_id);
            drop(coord); // Release the lock early
            first_step_id
        };
        
        let coord = coord_arc.lock().await;
        
        // Find the index of the source step in the pipeline
        let source_step_pipeline_index = coord.pipeline_steps.iter()
            .enumerate()
            .find_map(|(i, &step_idx)| {
                if coord.flow.steps[step_idx].id == source_step_id {
                    Some(i)
                } else {
                    None
                }
            });
        
        // Find the next step in the pipeline (if any)
        let target_step_id = if let Some(source_idx) = source_step_pipeline_index {
            if source_idx + 1 < coord.pipeline_steps.len() {
                // Get the next step in the pipeline
                let next_step_idx = coord.pipeline_steps[source_idx + 1];
                let next_step_id = coord.flow.steps[next_step_idx].id.clone();
                tracing::info!("Routing chunk from step {} to next step {}", source_step_id, next_step_id);
                next_step_id
            } else {
                // This is the last step in the pipeline, no forwarding needed
                tracing::info!("Step {} is the last in pipeline, no forwarding needed", source_step_id);
                return Ok(());
            }
        } else {
            // Couldn't find the source step in the pipeline, use the source step ID as fallback
            tracing::warn!("Could not find step {} in pipeline, using as target", source_step_id);
            source_step_id.clone()
        };
        
        // Send to the target step's channel
        if let Some(tx) = coord.step_senders.get(&target_step_id) {
            tracing::info!("[SEND-DEBUG] Found sender for target step {}, attempting to send chunk {} (tx addr: {:p})", 
                          target_step_id, chunk_index, tx);
            
            // Create a FlowResult from the chunk data
            let fr = FlowResult::Streaming {
                stream_id: map.get("stream_id")
                             .and_then(|v| v.as_str())
                             .unwrap_or("unknown")
                             .to_string(),
                metadata: stepflow_core::workflow::ValueRef::new(serde_json::json!(map)),
                chunk: map.get("chunk")
                         .and_then(|v| v.as_str())
                         .unwrap_or("")
                         .to_string(),
                chunk_index,
                is_final,
            };
            
            match tx.send(fr.clone()).await {
                Ok(_) => {
                    tracing::info!("[SEND-DEBUG] Successfully routed chunk {} from step {} to step {}", 
                                  chunk_index, source_step_id, target_step_id);
                }
                Err(e) => {
                    tracing::error!("[SEND-DEBUG] Failed to send chunk {} to step {}: {:?}", 
                                   chunk_index, target_step_id, e);
                    return Err(ExecutionError::StepFailed { step: target_step_id.clone() }.into());
                }
            }
        } else {
            tracing::warn!("[SEND-DEBUG] No channel for target step {} (available steps: {:?})", 
                          target_step_id, coord.step_senders.keys().collect::<Vec<_>>());
        }

        Ok(())
    }

    /// Called by `WorkflowExecutor::route_streaming_chunk` to inject
    /// *all* the chunks, not just the first.
    pub async fn handle_chunk(&mut self, chunk_json: serde_json::Value) -> Result<()> {
        let map = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(chunk_json)
            .map_err(|e| ExecutionError::MalformedReference { message: e.to_string() })?;
        
        // Extract the step ID this chunk belongs to (default to first pipeline step)
        let step_id = map.get("step_id")
                          .and_then(|v| v.as_str())
                          .map(|s| s.to_string())
                          .unwrap_or_else(|| {
                              // Default to first step (source step) if no step_id provided
                              // This handles chunks coming from Python components that don't include step_id
                              let first_step_idx = self.pipeline_steps[0];
                              let first_step_id = self.flow.steps[first_step_idx].id.clone();
                              tracing::info!("No step_id in chunk, defaulting to first pipeline step: {}", first_step_id);
                              first_step_id
                          });
                          
        let stream_id   = map.get("stream_id")  .and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let chunk_index = map.get("chunk_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let is_final    = map.get("is_final")   .and_then(|v| v.as_bool()).unwrap_or(false);

        // Build the FlowResult
        let fr = FlowResult::Streaming {
            stream_id: stream_id.clone(),
            metadata: stepflow_core::workflow::ValueRef::new(serde_json::Value::Object(map.clone())),
            chunk: map.get("chunk").and_then(|v|v.as_str()).unwrap_or("").to_string(),
            chunk_index,
            is_final,
        };

        // Only deliver to the step itself - let the step handle forwarding downstream
        if let Some(tx) = self.step_senders.get(&step_id) {
            tracing::info!("Found sender for step {}, attempting to send chunk {}", step_id, chunk_index);
            match tx.send(fr.clone()).await {
                Ok(_) => {
                    tracing::info!("Successfully routed chunk {} to step {} (pipeline steps: {:?})", chunk_index, step_id, self.pipeline_steps);
                }
                Err(e) => {
                    tracing::error!("Failed to route chunk {} to step {}: {:?}", chunk_index, step_id, e);
                    return Err(ExecutionError::StepFailed { step: step_id.clone() }.into());
                }
            }
        } else {
            tracing::warn!("No channel for step {} (available steps: {:?})", step_id, self.step_senders.keys().collect::<Vec<_>>());
        }

        Ok(())
    }

    fn new(
        executor: Arc<StepFlowExecutor>,
        flow: Arc<Flow>,
        pipeline_steps: Vec<usize>,
        context: ExecutionContext,
        resolver: ValueResolver,
    ) -> Self {
        let mut step_receivers = std::collections::HashMap::new();
        let mut step_downstream_senders = std::collections::HashMap::new();
        let mut step_senders = std::collections::HashMap::new();

        tracing::info!("[DEBUG-CHANNEL-SETUP] Creating channels for pipeline steps: {:?}", pipeline_steps);

        // Create input receivers for each step
        for &step_index in &pipeline_steps {
            let step_id = flow.steps[step_index].id.clone();
            let (input_tx, input_rx) = tokio::sync::mpsc::channel(100);
            let sender_clone = input_tx.clone();
            
            // Log channel creation with memory addresses
            tracing::info!(
                "[CHANNEL-DEBUG] Created channel for step {} (index {}): tx={:p}, rx={:p}", 
                step_id, 
                step_index, 
                &sender_clone as *const _,
                &input_rx as *const _
            );
            
            step_senders.insert(step_id.clone(), sender_clone);
            step_receivers.insert(step_id.clone(), input_rx);
        }

        // Set up the pipeline connections
        tracing::info!("[DEBUG-CHANNEL-SETUP] Setting up pipeline connections for {} steps", pipeline_steps.len());
        for i in 0..pipeline_steps.len() {
            let step_index = pipeline_steps[i];
            let step_id = flow.steps[step_index].id.clone();
            tracing::info!("[DEBUG-CHANNEL-SETUP] Processing step {} ({}) at position {}", step_id, step_index, i);
            
            // Set up downstream senders for this step
            let mut downstream_senders = Vec::new();
            if i < pipeline_steps.len() - 1 {
                // This step sends to the next step's input
                let next_step_index = pipeline_steps[i + 1];
                let next_step_id = flow.steps[next_step_index].id.clone();
                if let Some(next_step_sender) = step_senders.get(&next_step_id).cloned() {
                    downstream_senders.push(next_step_sender);
                }
            }
            step_downstream_senders.insert(step_id, downstream_senders);
        }

        Self {
            executor,
            flow,
            pipeline_steps,
            context,
            resolver,
            step_receivers,
            step_downstream_senders,
            step_senders,
        }
    }


    


    /// Run the pipeline without holding the mutex lock
    /// This allows route_streaming_chunk to acquire the lock while the pipeline is running
    async fn run_pipeline_without_lock(coord_arc: Arc<tokio::sync::Mutex<Self>>) -> Result<()> {
        tracing::info!("[DEBUG-PIPELINE] Starting pipeline execution without lock");
        
        // Resolve step inputs and spawn tasks while holding the lock to avoid race conditions
        let mut handles = Vec::new();
        {
            let mut guard = coord_arc.lock().await;
            let pipeline_steps = guard.pipeline_steps.clone();
            let executor = guard.executor.clone();
            let flow = guard.flow.clone();
            let context = guard.context.clone();
            
            // Resolve step inputs first
            let mut step_inputs = std::collections::HashMap::new();
            for &step_idx in &pipeline_steps {
                let input = guard.resolve_step_input(step_idx).await?;
                step_inputs.insert(step_idx, input);
            }
            
            // Now spawn all tasks while still holding the lock to prevent race conditions
            for &step_idx in &pipeline_steps {
                let step_id = flow.steps[step_idx].id.clone();
                
                // Take the receiver for this step - this is the correct approach
                // The sender stays in the coordinator so route_chunk_to_running_pipeline can send to it
                let rx = guard.step_receivers.remove(&step_id).ok_or_else(|| {
                    tracing::error!("[DEBUG-CHANNEL] No receiver found for step {}", step_id);
                    ExecutionError::Internal
                })?;
                tracing::info!("[DEBUG-CHANNEL] Moved receiver for step {} to task", step_id);
                let _sender = guard.step_senders.get(&step_id).unwrap().clone(); // Keep sender in map for handle_chunk
                let downstream = guard.step_downstream_senders
                    .get(&step_id).cloned().unwrap_or_default();
                
                tracing::info!("[DEBUG-CHANNEL] Step {} spawning with {} downstream channels", step_id, downstream.len());
                let input = step_inputs.remove(&step_idx).ok_or_else(|| {
                    ExecutionError::Internal
                })?;
                
                tracing::info!("Starting step task for {} with receiver while holding lock", step_id);
                let plugin = executor.get_plugin(&flow.steps[step_idx].component).await?;
                let context = context.clone()
                    .with_step(step_id.clone());
                let step = flow.steps[step_idx].clone();
                let is_source = step_idx == pipeline_steps[0];
                
                // Spawn while still holding the pieces and the lock
                let h = tokio::spawn(async move {
                    tracing::info!("Step task {} about to call run_streaming_step_simple", step_id);
                    let result = run_streaming_step_simple(plugin, step, input, context, rx, downstream, is_source).await;
                    tracing::info!("Step task {} finished run_streaming_step_simple: {:?}", step_id, result.is_ok());
                    result
                });
                handles.push((step_idx, h));
            }
        } // Lock dropped NOW - after all tasks are spawned with their receivers

        // Capture flow and pipeline_steps outside of the lock for later use
        let (flow, pipeline_steps) = {
            let guard = coord_arc.lock().await;
            (guard.flow.clone(), guard.pipeline_steps.clone())
        };
        
        // Give all tasks a moment to start
        tracing::info!("[DEBUG-PIPELINE] Giving tasks 500ms to start up");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        
        // Now trigger the source component to start generating chunks
        if let Some(source_step_idx) = pipeline_steps.first() {
            let source_step_id = flow.steps[*source_step_idx].id.clone();
            tracing::info!("[DEBUG-PIPELINE] Triggering source component {} to start generating", source_step_id);
            
            // Get the step input for the source step
            let source_input = {
                let guard = coord_arc.lock().await;
                guard.resolve_step_input(*source_step_idx).await?
            };
            
            // Get the plugin for the source step
            let source_plugin = {
                let guard = coord_arc.lock().await;
                guard.executor.get_plugin(&flow.steps[*source_step_idx].component).await?
            };
            
            // Create execution context for the source step
            let source_context = {
                let guard = coord_arc.lock().await;
                guard.context.clone().with_step(source_step_id.clone())
            };
            
            // Trigger the source component in a separate task (fire and forget)
            let source_step = flow.steps[*source_step_idx].clone();
            tokio::spawn(async move {
                tracing::info!("[DEBUG-GENERATOR] Starting source generator for {}", source_step_id);
                match execute_step_async(source_plugin, &source_step, source_input, source_context).await {
                    Ok(result) => {
                        tracing::info!("[DEBUG-GENERATOR] Source generator {} completed: {:?}", source_step_id, result);
                    }
                    Err(e) => {
                        tracing::error!("[DEBUG-GENERATOR] Source generator {} failed: {:?}", source_step_id, e);
                    }
                }
            });
        }
        
        // Wait for all step handles to complete
        tracing::info!("[DEBUG-PIPELINE] Waiting for all {} step handles to complete", handles.len());
        for (step_idx, handle) in handles {
            let step_id = &flow.steps[step_idx].id;
            tracing::info!("[DEBUG-PIPELINE] Waiting for step {} to complete", step_id);
            match handle.await {
                Ok(result) => {
                    if let Err(e) = result {
                        tracing::warn!("Step {} completed with error: {:?}", step_id, e);
                        return Err(e);
                    } else {
                        tracing::info!("Step {} completed successfully", step_id);
                    }
                }
                Err(e) => {
                    tracing::warn!("Step {} task panicked: {:?}", step_id, e);
                    return Err(ExecutionError::Internal.into());
                }
            }
        }
        
        tracing::info!("[DEBUG-PIPELINE] run_pipeline_without_lock completed successfully");
        Ok(())
    }

    async fn resolve_step_input(&self, step_index: usize) -> Result<stepflow_core::workflow::ValueRef> {
        // For streaming steps, we need simpler input resolution
        // since they don't depend on other steps' outputs
        let step = &self.flow.steps[step_index];
        
        // For streaming steps, resolve the input directly without dependencies
        // This avoids the "undefined value" error for streaming pipelines
        if step.streaming {
            // For streaming steps, try to resolve the input expression directly
            // If it fails, fall back to the workflow input
            match self.resolver.resolve(&step.input).await {
                Ok(FlowResult::Success { result }) => Ok(result),
                Ok(FlowResult::Streaming { metadata, .. }) => Ok(metadata),
                                 _ => {
                     // Fall back to workflow input for streaming steps
                     tracing::info!("[DEBUG-RESOLVE] Falling back to workflow input for streaming step {}", step.id);
                     Ok(self.resolver.workflow_input().clone())
                 }
            }
        } else {
            // For non-streaming steps, use the full resolver
            let step_input = match self.resolver.resolve(&step.input).await? {
                FlowResult::Success { result } => result,
                FlowResult::Streaming { metadata, .. } => metadata,
                FlowResult::Skipped => {
                    return Err(ExecutionError::StepNotRunnable {
                        step: step.id.clone(),
                    }
                    .into());
                }
                FlowResult::Failed { error } => {
                    return Err(ExecutionError::StepFailed { step: step.id.clone() }.into());
                }
            };
            Ok(step_input)
        }
    }


}

/// A per-step streaming loop: receive chunks, call your component, forward every chunk downstream,
/// exit only when `is_final == true`.
async fn run_streaming_step_simple(
    plugin: Arc<DynPlugin<'static>>,
    step: stepflow_core::workflow::Step,
    input: stepflow_core::workflow::ValueRef,
    context: ExecutionContext,
    mut rx: mpsc::Receiver<FlowResult>,
    downstream: Vec<mpsc::Sender<FlowResult>>,
    is_source: bool,
) -> Result<()> {
    let step_id = step.id.clone();
    stream_log!(info, &step_id, "starting (is_source={}, downstream={})", is_source, downstream.len());
    
    // For source steps, we now rely on the notification system to start the generator
    // The generator will be triggered when the first chunk request comes in
    if is_source {
        stream_log!(info, &step_id, "source step entering receiver loop, generator will start via notifications");
    } else {
        stream_log!(info, &step_id, "sink/middle step entering receiver loop");
    }

    // Now loop for all chunks coming through the coordinator's routing system
    let mut last_stream_id = String::new();
    let mut last_metadata = stepflow_core::workflow::ValueRef::new(serde_json::Value::Null);
    let mut last_chunk = String::new();
    let mut last_chunk_index = 0;
    let mut last_is_final = false;
    
    loop {
        tracing::info!("[STREAM] Step {} waiting for chunk via receiver", step_id);
        // Before waiting for data, log channel details
        tracing::info!(
            "[STREAM-DEBUG] Step {} waiting for chunk via receiver (rx addr: {:p})", 
            step_id, 
            &rx as *const _
        );

        // Check if the channel has been closed already
        if rx.is_closed() {
            stream_log!(warn, &step_id, "channel is already closed before receiving any data");
        }

        // Add timeout to prevent indefinite blocking
        let recv_result = match tokio::time::timeout(
            std::time::Duration::from_secs(10), // 10 second timeout
            rx.recv()
        ).await {
            Ok(result) => {
                tracing::info!(
                    "[STREAM-DEBUG] Step {} received data from channel: is_some={}", 
                    step_id, 
                    result.is_some()
                );
                result
            },
            Err(_) => {
                stream_log!(warn, &step_id, "TIMEOUT waiting for chunk after 10 seconds");
                // Continue with loop to try again or return None to exit
                None
            }
        };

        match recv_result {
            Some(FlowResult::Streaming { stream_id, metadata, chunk, chunk_index, is_final }) => {
                stream_log!(info, &step_id, "RECEIVED chunk #{} from receiver", chunk_index);
                tracing::info!("RX step={} idx={} final={} (source={})", step_id, chunk_index, is_final, is_source);
                stream_log!(info, &step_id, "processing chunk #{}", chunk_index);
                
                // Store the streaming metadata for potential use in non-streaming case
                last_stream_id = stream_id.clone();
                last_metadata = metadata.clone();
                last_chunk = chunk.clone();
                last_chunk_index = chunk_index;
                last_is_final = is_final;
                
                // For non-source steps, call the component to process the chunk
                let (final_stream_id, final_metadata, final_chunk, final_chunk_index, final_is_final) = if !is_source {
                    // For non-source steps, we need to create a proper input that contains only the actual data
                    // without any $from references, by extracting the relevant fields from the streaming chunk
                    let chunk_input_data = serde_json::json!({
                        "chunk": chunk,
                        "stream_id": stream_id,
                        "chunk_index": chunk_index,
                        "is_final": is_final,
                        // Add all the metadata fields from the streaming chunk
                        "sample_rate": metadata.as_ref().get("sample_rate").unwrap_or(&serde_json::Value::Null),
                        "channels": metadata.as_ref().get("channels").unwrap_or(&serde_json::Value::Null),
                        "operation": metadata.as_ref().get("operation").unwrap_or(&serde_json::Value::Null),
                        "output_file": metadata.as_ref().get("output_file").unwrap_or(&serde_json::Value::Null),
                        "gain": metadata.as_ref().get("gain").unwrap_or(&serde_json::Value::Null),
                    });
                    let chunk_input = stepflow_core::workflow::ValueRef::new(chunk_input_data);
                    
                    // Call the component to process the chunk
                    let step_context = context.clone().with_step(step.id.clone());
                    match execute_step_async(plugin.clone(), &step, chunk_input, step_context).await? {
                        FlowResult::Streaming { stream_id: processed_stream_id, metadata: processed_metadata, chunk: processed_chunk, chunk_index: processed_chunk_index, is_final: processed_is_final } => {
                            tracing::info!("[STREAM] Step {} component processed chunk #{}", step_id, processed_chunk_index);
                            (processed_stream_id, processed_metadata, processed_chunk, processed_chunk_index, processed_is_final)
                        }
                        FlowResult::Success { .. } => {
                            tracing::info!("[STREAM] Step {} component returned success, forwarding original chunk", step_id);
                            // For success results, forward the original chunk
                            (stream_id, metadata, chunk, chunk_index, is_final)
                        }
                        other => {
                            tracing::warn!("[STREAM] Step {} component returned unexpected result: {:?}", step_id, other);
                            (stream_id, metadata, chunk, chunk_index, is_final)
                        }
                    }
                } else {
                    // Source step just forwards the chunk as-is
                    (stream_id, metadata, chunk, chunk_index, is_final)
                };
                
                // Forward to downstream steps
                stream_log!(info, &step_id, "forwarding chunk #{} to {} downstream", final_chunk_index, downstream.len());
                if downstream.is_empty() {
                    stream_log!(warn, &step_id, "no downstream channels to forward to!");
                }
                for (i, tx) in downstream.iter().enumerate() {
                    match tx.send(FlowResult::Streaming {
                        stream_id: final_stream_id.clone(),
                        metadata: final_metadata.clone(),
                        chunk: final_chunk.clone(),
                        chunk_index: final_chunk_index,
                        is_final: final_is_final,
                    }).await {
                        Ok(_) => {
                            stream_log!(info, &step_id, "successfully forwarded chunk #{} to downstream {}", final_chunk_index, i);
                        }
                        Err(e) => {
                            stream_log!(warn, &step_id, "failed to forward chunk #{} to downstream {}: {:?}", final_chunk_index, i, e);
                        }
                    }
                }

                // Stop the *source* task once it sees its own final packet.
                // Every other task keeps listening until its inbound channel is closed.
                if is_source && final_is_final {
                    stream_log!(info, &step_id, "saw final chunk, exiting");
                    break;
                }
            }
            Some(other) => {
                stream_log!(warn, &step_id, "received non-streaming result: {:?}", other);
                // Extract actual data from the non-streaming result and forward it as streaming
                match other {
                    FlowResult::Success { result } => {
                        // Convert the success result to a streaming chunk
                        for tx in &downstream {
                            let _ = tx.send(FlowResult::Streaming {
                                stream_id: last_stream_id.clone(),
                                metadata: result.clone(),
                                chunk: serde_json::to_string(result.as_ref()).unwrap_or_default(),
                                chunk_index: last_chunk_index,
                                is_final: last_is_final,
                            }).await;
                        }
                    }
                    _ => {
                        // For other result types, use last known metadata
                        for tx in &downstream {
                            let _ = tx.send(FlowResult::Streaming {
                                stream_id: last_stream_id.clone(),
                                metadata: last_metadata.clone(),
                                chunk: last_chunk.clone(),
                                chunk_index: last_chunk_index,
                                is_final: last_is_final,
                            }).await;
                        }
                    }
                }
                // Only exit if this was truly the final chunk
                if last_is_final {
                    stream_log!(info, &step_id, "received final packet in non-streaming arm, exiting");
                    break;
                }
                // Otherwise keep looping to process more chunks
            }
            None => {
                stream_log!(info, &step_id, "channel closed");
                break;
            }
        }
    }
    
    stream_log!(info, &step_id, "completed");
    Ok(())
}

/// Sort streaming steps by their dependencies using a topological sort
/// This ensures that source steps come before steps that depend on them
fn sort_streaming_steps_by_dependencies(
    flow: &Flow,
    streaming_steps: Vec<usize>,
) -> Result<Vec<usize>> {
    use std::collections::{HashMap, HashSet, VecDeque};
    
    // Create a map of step ID to index for quick lookup
    let step_id_to_index: HashMap<String, usize> = streaming_steps
        .iter()
        .map(|&idx| (flow.steps[idx].id.clone(), idx))
        .collect();
    
    // Build dependency graph for streaming steps only
    let mut dependencies: HashMap<usize, HashSet<usize>> = HashMap::new();
    let mut dependents: HashMap<usize, HashSet<usize>> = HashMap::new();
    
    for &step_idx in &streaming_steps {
        dependencies.insert(step_idx, HashSet::new());
        dependents.insert(step_idx, HashSet::new());
    }
    
    // Analyze dependencies between streaming steps
    for &step_idx in &streaming_steps {
        let step = &flow.steps[step_idx];
        
        // Check if this step's input references other streaming steps
        let input_str = serde_json::to_string(&step.input).unwrap_or_default();
        
        for &other_step_idx in &streaming_steps {
            if step_idx != other_step_idx {
                let other_step_id = &flow.steps[other_step_idx].id;
                
                // Check if step references other_step in its input
                if input_str.contains(&format!("step: {}", other_step_id)) ||
                   input_str.contains(&format!("\"step\": \"{}\"", other_step_id)) {
                    // step_idx depends on other_step_idx
                    dependencies.get_mut(&step_idx).unwrap().insert(other_step_idx);
                    dependents.get_mut(&other_step_idx).unwrap().insert(step_idx);
                    
                    tracing::info!("Detected dependency: {} depends on {}", 
                                 step.id, other_step_id);
                }
            }
        }
    }
    
    // Topological sort using Kahn's algorithm
    let mut result = Vec::new();
    let mut queue = VecDeque::new();
    let mut remaining_deps = dependencies.clone();
    
    // Find steps with no dependencies (source steps)
    for &step_idx in &streaming_steps {
        if remaining_deps[&step_idx].is_empty() {
            queue.push_back(step_idx);
            tracing::info!("Found source streaming step: {}", flow.steps[step_idx].id);
        }
    }
    
    while let Some(current_step) = queue.pop_front() {
        result.push(current_step);
        
        // Remove this step from its dependents' dependency lists
        for &dependent_step in &dependents[&current_step] {
            remaining_deps.get_mut(&dependent_step).unwrap().remove(&current_step);
            
            // If the dependent now has no dependencies, add it to the queue
            if remaining_deps[&dependent_step].is_empty() {
                queue.push_back(dependent_step);
            }
        }
    }
    
    // Check for circular dependencies
    if result.len() != streaming_steps.len() {
        let remaining: Vec<String> = streaming_steps
            .iter()
            .filter(|&&idx| !result.contains(&idx))
            .map(|&idx| flow.steps[idx].id.clone())
            .collect();
        
        tracing::error!("Circular dependency detected in streaming steps: {:?}", remaining);
        return Err(ExecutionError::Internal.into());
    }
    
    tracing::info!("Topological sort result: {:?}",
        result.iter().map(|i| &flow.steps[*i].id).collect::<Vec<_>>()
    );
    
    Ok(result)
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
    ) {
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

        let flow_hash = Flow::hash(flow.as_ref());
        (executor, flow, flow_hash)
    }

    /// Execute a workflow from YAML string with given input
    pub async fn execute_workflow_from_yaml_simple(
        yaml_str: &str,
        input: serde_json::Value,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> Result<FlowResult> {
        let (executor, flow, workflow_hash) =
            create_workflow_from_yaml_simple(yaml_str, mock_behaviors).await;
        let execution_id = Uuid::new_v4();
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let input_ref = ValueRef::new(input);

        execute_workflow(
            executor,
            flow,
            workflow_hash,
            execution_id,
            input_ref,
            state_store,
        )
        .await
    }

    /// Create a WorkflowExecutor from YAML string for step-by-step testing
    pub async fn create_workflow_executor_from_yaml_simple(
        yaml_str: &str,
        input: serde_json::Value,
        mock_behaviors: Vec<(&str, FlowResult)>,
    ) -> Result<WorkflowExecutor> {
        let (executor, flow, workflow_hash) =
            create_workflow_from_yaml_simple(yaml_str, mock_behaviors).await;
        let execution_id = Uuid::new_v4();
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let input_ref = ValueRef::new(input);

        WorkflowExecutor::new(
            executor,
            flow,
            workflow_hash,
            execution_id,
            input_ref,
            state_store,
        )
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

    fn create_test_flow(steps: Vec<Step>, output: ValueRef) -> Flow {
        Flow {
            name: None,
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps,
            output,
            test: None,
            examples: vec![],
        }
    }

    #[tokio::test]
    async fn test_dependency_tracking_basic() {
        // Test that we can create dependencies and tracker correctly
        let steps = vec![
            create_test_step("step1", json!({"value": 42})),
            create_test_step("step2", json!({"$from": {"step": "step1"}})),
        ];

        let flow = create_test_flow(steps, json!({"$from": {"step": "step2"}}).into());

        // Build dependencies using analysis crate
        let analysis = stepflow_analysis::analyze_workflow_dependencies(
            std::sync::Arc::new(flow),
            "test_hash".into(),
        )
        .unwrap();

        // Create tracker from analysis
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

        // Check the final result
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
        match result.result {
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
        match result.result {
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
    onError:
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
    onError:
      action: useDefault
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
    onError:
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
    onError:
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
    onError:
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

