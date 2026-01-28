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

//! Step execution logic.
//!
//! [`StepRunner`] encapsulates all the setup needed to execute a step:
//! - Plugin lookup
//! - Execution context creation
//! - Input handling
//! - Error conversion
//!
//! This is shared between [`FlowExecutor`](crate::FlowExecutor) and
//! [`DebugExecutor`](crate::DebugExecutor).

use std::sync::Arc;

use error_stack::ResultExt as _;
use serde::Serialize;
use stepflow_core::workflow::{Flow, Step, StepId, ValueRef};
use stepflow_core::{BlobId, FlowResult};
use stepflow_observability::StepIdGuard;
use stepflow_plugin::{
    DynPlugin, ExecutionContext, Plugin as _, PluginRouterExt as _, RunContext, StepflowEnvironment,
};

use crate::{ExecutionError, Result};

// ============================================================================
// Result Types
// ============================================================================

/// Metadata about a step execution.
///
/// This provides identifying information for a step without the result.
#[derive(Debug, Clone)]
pub struct StepMetadata {
    /// Step identifier (index + name).
    pub step: StepId,
    /// Component path (e.g., "/builtin/openai").
    pub component: String,
}

// Custom serialization - step_id is the only identifier exposed in the API
impl Serialize for StepMetadata {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct as _;
        let mut state = serializer.serialize_struct("StepMetadata", 2)?;
        state.serialize_field("step_id", &self.step.name())?;
        state.serialize_field("component", &self.component)?;
        state.end()
    }
}

// Note: StepMetadata intentionally does NOT implement Deserialize.
// The step index would be lost or default to 0, causing potential bugs.
// If deserialization is needed, use a separate DTO type.

/// Result of executing a step, including metadata.
///
/// This is the result type returned by `StepRunner` and used by
/// [`FlowExecutor`](crate::FlowExecutor).
// Note: StepRunResult intentionally does NOT implement Deserialize.
// The step index would be lost or default to 0, causing potential bugs.
#[derive(Debug, Clone, Serialize)]
pub struct StepRunResult {
    /// Step metadata (index, id, component).
    #[serde(flatten)]
    pub metadata: StepMetadata,
    /// The execution result (success or failure).
    pub result: FlowResult,
}

impl StepRunResult {
    /// Create a new step run result from a StepId.
    pub fn new(step_id: StepId, component: String, result: FlowResult) -> Self {
        Self {
            metadata: StepMetadata {
                step: step_id,
                component,
            },
            result,
        }
    }

    /// Get the step index.
    pub fn step_index(&self) -> usize {
        self.metadata.step.index()
    }

    /// Get the step name.
    pub fn step_name(&self) -> &str {
        self.metadata.step.name()
    }

    /// Get the StepId.
    pub fn step_id(&self) -> &StepId {
        &self.metadata.step
    }

    /// Get the component path.
    pub fn component(&self) -> &str {
        &self.metadata.component
    }

    /// Check if the result is a success.
    pub fn is_success(&self) -> bool {
        matches!(self.result, FlowResult::Success(_))
    }

    /// Check if the result is a failure.
    pub fn is_failed(&self) -> bool {
        matches!(self.result, FlowResult::Failed(_))
    }
}

/// Execute a single step asynchronously.
///
/// This is the core step execution function used by `StepRunner`.
/// It handles:
/// - Plugin execution
/// - Error handling (useDefault, fail, retry)
/// - Observability (tracing spans, logging)
async fn execute_step_async(
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
        let result = match plugin.execute(&component, context, input).await {
            Ok(result) => result,
            Err(error) => {
                // Convert plugin execution errors to FlowResult::Failed
                let flow_error = stepflow_core::FlowError::from_error_stack(
                    error
                        .change_context(ExecutionError::StepFailed {
                            step: step.id.to_owned(),
                        })
                        .attach_printable(format!(
                            "Component execution failed for step '{}'",
                            step.id
                        ))
                        .attach_printable(format!("Component: {resolved_component}")),
                );
                FlowResult::Failed(flow_error)
            }
        };

        log::debug!("Step execution completed: step_id={}", step.id);

        Ok::<_, error_stack::Report<ExecutionError>>(result)
    }
    .in_span(span)
    .await?;

    match &result {
        FlowResult::Failed(error) => match step.on_error_or_default() {
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
        },
        _ => Ok(result),
    }
}

/// Executes a single step within a workflow.
///
/// `StepRunner` encapsulates all the setup needed to execute a step:
/// - Plugin lookup
/// - Execution context creation
/// - Input handling
/// - Error conversion
///
/// # Example
///
/// ```ignore
/// let runner = StepRunner::new(
///     flow.clone(),
///     step_index,
///     step_input,
///     stepflow.clone(),
///     flow_id,
///     run_context,
/// );
///
/// let result = runner.run().await;
/// ```
pub struct StepRunner {
    /// The step to execute.
    step: Step,
    /// Index of the step in the flow.
    step_index: usize,
    /// The flow containing this step.
    flow: Arc<Flow>,
    /// Resolved step input (may be Success or Failed).
    step_input: FlowResult,
    /// Environment for plugin access.
    env: Arc<StepflowEnvironment>,
    /// Flow ID for execution context.
    flow_id: BlobId,
    /// Run context for bidirectional communication (run hierarchy, subflow submission).
    run_context: Arc<RunContext>,
}

impl StepRunner {
    /// Create a new step runner.
    ///
    /// The `step_input` can be either `FlowResult::Success` (resolved input)
    /// or `FlowResult::Failed` (input resolution failed). If failed, `run()`
    /// will return the failure immediately.
    ///
    /// The `run_context` provides run hierarchy information (run_id, root_run_id)
    /// and optionally a subflow submitter for in-process subflow execution.
    pub fn new(
        flow: Arc<Flow>,
        step_index: usize,
        step_input: FlowResult,
        env: Arc<StepflowEnvironment>,
        flow_id: BlobId,
        run_context: Arc<RunContext>,
    ) -> Self {
        let step = flow.step(step_index).clone();
        Self {
            step,
            step_index,
            flow,
            step_input,
            env,
            flow_id,
            run_context,
        }
    }

    /// Execute the step and return the result.
    ///
    /// Returns:
    /// - `Ok(StepRunResult)` with `FlowResult::Success` - step completed successfully
    /// - `Ok(StepRunResult)` with `FlowResult::Failed` - business logic failure
    ///   (input resolution failed, step returned error, etc.)
    /// - `Err(...)` - infrastructure/system error (plugin lookup failed, network error, etc.)
    ///
    /// Callers should handle these differently:
    /// - Business failures (`FlowResult::Failed`) are expected and should be recorded
    /// - System errors (`Err`) indicate infrastructure issues and may warrant logging/alerting
    pub async fn run(self) -> Result<StepRunResult> {
        let step_id = StepId::for_step(self.flow.clone(), self.step_index);
        let component = self.step.component.to_string();

        // Handle input resolution failure (business logic failure)
        let input = match self.step_input {
            FlowResult::Success(value) => value,
            FlowResult::Failed(error) => {
                return Ok(StepRunResult::new(
                    step_id,
                    component,
                    FlowResult::Failed(error),
                ));
            }
        };

        // Get plugin for this component (system error if fails)
        let (plugin, resolved_component) = self
            .env
            .get_plugin_and_component(&self.step.component, input.clone())
            .change_context(ExecutionError::RouterError)?;

        // Create execution context with run context for bidirectional communication
        let context = ExecutionContext::for_step(
            self.env.clone(),
            step_id.clone(),
            self.flow_id,
            self.run_context,
        );

        // Execute the step (system error if infrastructure fails)
        let result =
            execute_step_async(plugin, &self.step, &resolved_component, input, context).await?;

        Ok(StepRunResult::new(step_id, component, result))
    }
}
