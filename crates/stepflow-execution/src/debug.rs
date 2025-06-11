use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_core::{
    FlowResult,
    status::StepStatus as CoreStepStatus,
    workflow::{Flow, ValueRef},
};
use stepflow_state::StateStore;
use uuid::Uuid;

use crate::{ExecutionError, Result, StepFlowExecutor, WorkflowExecutor};

/// A debugging session for step-by-step workflow execution.
///
/// This is built on top of WorkflowExecutor and provides debugging-specific
/// functionality while leveraging the core execution engine.
pub struct DebugSession {
    /// Core workflow executor that handles the actual execution logic
    executor: WorkflowExecutor,
}

impl DebugSession {
    /// Create a new debug session for the given workflow and input.
    pub async fn new(
        executor: Arc<StepFlowExecutor>,
        flow: Arc<Flow>,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
    ) -> Result<Self> {
        let execution_id = Uuid::new_v4();
        Self::new_with_execution_id(executor, flow, input, state_store, execution_id).await
    }

    /// Create a new debug session with a specific execution ID.
    pub async fn new_with_execution_id(
        executor: Arc<StepFlowExecutor>,
        flow: Arc<Flow>,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
        execution_id: Uuid,
    ) -> Result<Self> {
        let workflow_executor =
            WorkflowExecutor::new(executor, flow, execution_id, input, state_store)?;

        Ok(Self {
            executor: workflow_executor,
        })
    }

    /// Get the execution ID for this debug session.
    pub fn execution_id(&self) -> Uuid {
        self.executor.execution_id()
    }

    /// List all steps in the workflow with their current status.
    pub async fn list_all_steps(&self) -> Vec<StepStatus> {
        let runnable = self.executor.get_runnable_step_indices();

        let mut step_statuses = Vec::new();
        for (idx, step) in self.executor.flow().steps.iter().enumerate() {
            let state = if runnable.contains(idx) {
                CoreStepStatus::Runnable
            } else {
                // Check if step is completed by querying state store
                match self
                    .executor
                    .state_store()
                    .get_step_result_by_index(self.executor.execution_id(), idx)
                    .await
                {
                    Ok(result) => match result {
                        FlowResult::Success { .. } => CoreStepStatus::Completed,
                        FlowResult::Skipped => CoreStepStatus::Skipped,
                        FlowResult::Failed { .. } => CoreStepStatus::Failed,
                    },
                    Err(_) => CoreStepStatus::Blocked,
                }
            };

            step_statuses.push(StepStatus {
                index: idx,
                id: step.id.clone(),
                component: step.component.to_string(),
                state,
            });
        }

        step_statuses
    }

    /// Get currently runnable steps.
    pub fn get_runnable_steps(&self) -> Vec<StepStatus> {
        let runnable = self.executor.get_runnable_step_indices();

        runnable
            .iter()
            .map(|idx| {
                let step = &self.executor.flow().steps[idx];
                StepStatus {
                    index: idx,
                    id: step.id.clone(),
                    component: step.component.to_string(),
                    state: CoreStepStatus::Runnable,
                }
            })
            .collect()
    }

    /// Execute a specific step by ID.
    /// Returns an error if the step is not currently runnable.
    pub async fn execute_step(&mut self, step_id: &str) -> Result<FlowResult> {
        // Find the step by ID
        let step_index = self
            .executor
            .flow()
            .steps
            .iter()
            .position(|step| step.id == step_id)
            .ok_or_else(|| ExecutionError::StepNotFound {
                step: step_id.to_string(),
            })?;

        // Check if the step is runnable
        let runnable = self.executor.get_runnable_step_indices();
        if !runnable.contains(step_index) {
            return Err(ExecutionError::StepNotRunnable {
                step: step_id.to_string(),
            }
            .into());
        }

        self.executor.execute_step_by_index(step_index).await
    }

    /// Execute multiple steps by their IDs.
    /// Returns execution results for each step.
    /// Only executes steps that are currently runnable.
    /// Steps are executed sequentially for better debugging traceability.
    pub async fn execute_multiple_steps(
        &mut self,
        step_ids: &[String],
    ) -> Result<Vec<StepExecutionResult>> {
        let mut results = Vec::new();

        // Execute sequentially to maintain simplicity and avoid complex
        // dependency tracking across parallel executions. The main workflow executor
        // already handles parallel execution optimally when running complete workflows.
        // This debug mode is primarily for step-by-step debugging where sequential
        // execution provides clearer understanding of workflow progression.
        for step_id in step_ids {
            match self.execute_step(step_id).await {
                Ok(flow_result) => {
                    results.push(StepExecutionResult::success(step_id.clone(), flow_result));
                }
                Err(e) => {
                    results.push(StepExecutionResult::failed(step_id.clone(), e.to_string()));
                }
            }
        }

        Ok(results)
    }

    /// Execute all currently runnable steps.
    /// Returns execution results for each step.
    pub async fn execute_all_runnable(&mut self) -> Result<Vec<StepExecutionResult>> {
        let runnable_steps = self.get_runnable_steps();
        let step_ids: Vec<String> = runnable_steps.iter().map(|s| s.id.clone()).collect();
        self.execute_multiple_steps(&step_ids).await
    }

    /// Get all completed steps with their results.
    pub async fn get_completed_steps(&self) -> Result<Vec<CompletedStep>> {
        let step_results = self
            .executor
            .state_store()
            .list_step_results(self.executor.execution_id())
            .await
            .change_context(ExecutionError::StateError)?;

        let completed_steps = step_results
            .into_iter()
            .map(|step_result| CompletedStep {
                index: step_result.step_idx(),
                id: step_result.step_id().to_string(),
                component: self.executor.flow().steps[step_result.step_idx()]
                    .component
                    .to_string(),
                result: step_result.into_result(),
            })
            .collect();

        Ok(completed_steps)
    }

    /// Get the output/result of a specific step by ID.
    pub async fn get_step_output(&self, step_id: &str) -> Result<FlowResult> {
        self.executor
            .state_store()
            .get_step_result_by_id(self.executor.execution_id(), step_id)
            .await
            .attach_printable_lazy(|| format!("Failed to get output for step '{}'", step_id))
            .change_context(ExecutionError::StepNotFound {
                step: step_id.to_string(),
            })
    }

    /// Continue execution until completion.
    /// Returns a tuple of (executed_steps, final_result).
    pub async fn continue_to_end(&mut self) -> Result<(Vec<(String, FlowResult)>, FlowResult)> {
        let mut executed_steps = Vec::new();

        // Keep executing until no steps are runnable
        loop {
            let runnable = self.executor.get_runnable_step_indices();

            if runnable.is_empty() {
                break;
            }

            // Execute all currently runnable steps
            for step_index in runnable.iter() {
                let step_id = self.executor.flow().steps[step_index].id.clone();
                let result = self.executor.execute_step_by_index(step_index).await?;
                executed_steps.push((step_id, result));
            }
        }

        // Resolve the final workflow output
        let final_result = self.executor.resolve_workflow_output().await?;

        Ok((executed_steps, final_result))
    }

    /// Get the details of a specific step.
    pub async fn inspect_step(&self, step_id: &str) -> Result<StepInspection> {
        let step_index = self
            .executor
            .flow()
            .steps
            .iter()
            .position(|step| step.id == step_id)
            .ok_or_else(|| ExecutionError::StepNotFound {
                step: step_id.to_string(),
            })?;

        let step = &self.executor.flow().steps[step_index];
        let runnable = self.executor.get_runnable_step_indices();

        let state = if runnable.contains(step_index) {
            CoreStepStatus::Runnable
        } else {
            // Check if step is completed by querying state store
            match self
                .executor
                .state_store()
                .get_step_result_by_index(self.executor.execution_id(), step_index)
                .await
            {
                Ok(result) => match result {
                    FlowResult::Success { .. } => CoreStepStatus::Completed,
                    FlowResult::Skipped => CoreStepStatus::Skipped,
                    FlowResult::Failed { .. } => CoreStepStatus::Failed,
                },
                Err(_) => CoreStepStatus::Blocked,
            }
        };

        Ok(StepInspection {
            index: step_index,
            id: step.id.clone(),
            component: step.component.to_string(),
            input: step.input.clone(),
            skip_if: step.skip_if.clone(),
            on_error: step.on_error.clone(),
            state,
        })
    }
}


/// Status information for a step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct StepStatus {
    pub index: usize,
    pub id: String,
    pub component: String,
    pub state: CoreStepStatus,
}

/// Detailed inspection information for a step.
#[derive(Debug, Clone)]
pub struct StepInspection {
    pub index: usize,
    pub id: String,
    pub component: String,
    pub input: ValueRef,
    pub skip_if: Option<stepflow_core::workflow::Expr>,
    pub on_error: stepflow_core::workflow::ErrorAction,
    pub state: CoreStepStatus,
}

/// Information about a completed step.
#[derive(Debug, Clone)]
pub struct CompletedStep {
    pub index: usize,
    pub id: String,
    pub component: String,
    pub result: FlowResult,
}

/// Result of executing a step in debug mode.
#[derive(Debug, Clone)]
pub struct StepExecutionResult {
    pub step_id: String,
    pub result: FlowResult,
    pub error: Option<String>,
}

impl StepExecutionResult {
    pub fn success(step_id: String, result: FlowResult) -> Self {
        Self {
            step_id,
            result,
            error: None,
        }
    }

    pub fn failed(step_id: String, error: String) -> Self {
        Self {
            step_id,
            result: FlowResult::Failed {
                error: stepflow_core::FlowError::new(500, error.clone()),
            },
            error: Some(error),
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}
