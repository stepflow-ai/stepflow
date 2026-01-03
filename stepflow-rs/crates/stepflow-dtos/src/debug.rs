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

//! Debug-related data transfer objects.
//!
//! These types are shared between `stepflow-state` (for persistence) and
//! `stepflow-execution` (for runtime usage).

use serde::{Deserialize, Serialize};
use stepflow_core::status::StepStatus;
use stepflow_core::{FlowResult, values::ValueRef};
use utoipa::ToSchema;
use uuid::Uuid;

/// Events that occur during debug execution.
///
/// These are persisted to enable session recovery and provide an audit trail
/// of what happened during debugging.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DebugEvent {
    /// A step was queued for execution (added to needed set).
    StepQueued {
        /// Index of the step in the flow.
        step_index: usize,
        /// Name/ID of the step.
        step_id: String,
    },

    /// A step started executing.
    StepStarted {
        /// Index of the step in the flow.
        step_index: usize,
        /// Name/ID of the step.
        step_id: String,
        /// The resolved input passed to the step.
        input: ValueRef,
    },

    /// A step completed successfully or with a handled error.
    StepCompleted {
        /// Index of the step in the flow.
        step_index: usize,
        /// Name/ID of the step.
        step_id: String,
        /// The result of the step execution.
        result: FlowResult,
    },

    /// A step failed (infrastructure/unhandled error).
    StepFailed {
        /// Index of the step in the flow.
        step_index: usize,
        /// Name/ID of the step.
        step_id: String,
        /// Error message describing the failure.
        error: String,
    },

    /// The run completed (all items finished).
    RunCompleted {
        /// The run that completed.
        run_id: Uuid,
        /// Whether the run succeeded or failed.
        success: bool,
    },
}

impl DebugEvent {
    /// Create a StepQueued event.
    pub fn step_queued(step_index: usize, step_id: impl Into<String>) -> Self {
        Self::StepQueued {
            step_index,
            step_id: step_id.into(),
        }
    }

    /// Create a StepStarted event.
    pub fn step_started(step_index: usize, step_id: impl Into<String>, input: ValueRef) -> Self {
        Self::StepStarted {
            step_index,
            step_id: step_id.into(),
            input,
        }
    }

    /// Create a StepCompleted event.
    pub fn step_completed(
        step_index: usize,
        step_id: impl Into<String>,
        result: FlowResult,
    ) -> Self {
        Self::StepCompleted {
            step_index,
            step_id: step_id.into(),
            result,
        }
    }

    /// Create a StepFailed event.
    pub fn step_failed(step_index: usize, step_id: impl Into<String>, error: String) -> Self {
        Self::StepFailed {
            step_index,
            step_id: step_id.into(),
            error,
        }
    }

    /// Create a RunCompleted event.
    pub fn run_completed(run_id: Uuid, success: bool) -> Self {
        Self::RunCompleted { run_id, success }
    }

    /// Get the step index if this event is related to a step.
    pub fn step_index(&self) -> Option<usize> {
        match self {
            Self::StepQueued { step_index, .. }
            | Self::StepStarted { step_index, .. }
            | Self::StepCompleted { step_index, .. }
            | Self::StepFailed { step_index, .. } => Some(*step_index),
            Self::RunCompleted { .. } => None,
        }
    }

    /// Get the step ID if this event is related to a step.
    pub fn step_id(&self) -> Option<&str> {
        match self {
            Self::StepQueued { step_id, .. }
            | Self::StepStarted { step_id, .. }
            | Self::StepCompleted { step_id, .. }
            | Self::StepFailed { step_id, .. } => Some(step_id),
            Self::RunCompleted { .. } => None,
        }
    }
}

/// The next action pending in the debug session.
///
/// This represents what the debugger is waiting for or what will happen
/// on the next "step" command.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PendingAction {
    /// Ready to execute a specific step.
    ExecuteStep {
        /// Index of the step in the flow.
        step_index: usize,
        /// Name/ID of the step.
        step_id: String,
    },

    /// No steps are ready; waiting for the user to queue steps.
    #[default]
    AwaitingInput,

    /// The run has completed (no more actions).
    Complete,
}

impl PendingAction {
    /// Create an ExecuteStep action.
    pub fn execute_step(step_index: usize, step_id: impl Into<String>) -> Self {
        Self::ExecuteStep {
            step_index,
            step_id: step_id.into(),
        }
    }

    /// Check if this is the Complete state.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }

    /// Check if this is an ExecuteStep action.
    pub fn is_execute_step(&self) -> bool {
        matches!(self, Self::ExecuteStep { .. })
    }

    /// Check if this is the AwaitingInput state.
    pub fn is_awaiting_input(&self) -> bool {
        matches!(self, Self::AwaitingInput)
    }
}

// ============================================================================
// Debug API DTOs
// ============================================================================

/// Brief information about a step (used in step listings).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepInfo {
    /// Index of the step in the flow.
    pub step_index: usize,
    /// Name/ID of the step.
    pub step_id: String,
    /// Component path (e.g., "/builtin/openai").
    pub component: String,
    /// Current status of the step.
    pub status: StepStatus,
}

impl StepInfo {
    /// Create a new StepInfo.
    pub fn new(
        step_index: usize,
        step_id: impl Into<String>,
        component: impl Into<String>,
        status: StepStatus,
    ) -> Self {
        Self {
            step_index,
            step_id: step_id.into(),
            component: component.into(),
            status,
        }
    }
}

/// Detailed information about a step (used for `info <step_id>`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepDetail {
    /// Basic step info.
    #[serde(flatten)]
    pub info: StepInfo,
    /// The input expression for the step.
    #[schema(value_type = Object)]
    pub input: stepflow_core::ValueExpr,
    /// Error handling configuration.
    #[schema(value_type = String)]
    pub on_error: stepflow_core::workflow::ErrorAction,
    /// The result if the step has completed.
    pub result: Option<FlowResult>,
    /// IDs of steps this step depends on.
    pub dependencies: Vec<String>,
}

/// Status filter for step queries.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepStatusFilter {
    /// Return all steps.
    #[default]
    All,
    /// Return only completed steps.
    Completed,
    /// Return only pending (needed but not completed) steps.
    Pending,
    /// Return only runnable (ready to execute) steps.
    Runnable,
    /// Return only blocked steps.
    Blocked,
}

/// Current debug session status.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugStatus {
    /// The run ID being debugged.
    pub run_id: Uuid,
    /// The current run context (may differ from run_id when in a sub-flow).
    pub current_run_id: Uuid,
    /// The pending action (what happens on next step/continue).
    pub pending: PendingAction,
    /// Steps that are ready to execute.
    pub ready_steps: Vec<String>,
    /// Number of completed steps.
    pub completed_count: usize,
    /// Total number of steps in the flow.
    pub total_steps: usize,
}

/// Result of executing a step via next/step commands.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepExecutionResult {
    /// The step that was executed (if any).
    pub step_id: Option<String>,
    /// The result of execution (if a step was executed).
    pub result: Option<FlowResult>,
    /// Updated debug status after execution.
    pub status: DebugStatus,
}

/// Result of continuing execution to completion.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContinueResult {
    /// The final result of the workflow.
    pub result: FlowResult,
    /// Number of steps executed.
    pub steps_executed: usize,
    /// Updated debug status (should be Complete).
    pub status: DebugStatus,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_debug_event_serialization() {
        let event = DebugEvent::step_queued(0, "step1");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("step_queued"));
        assert!(json.contains("step1"));

        let deserialized: DebugEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_debug_event_step_started() {
        let input = ValueRef::new(json!({"key": "value"}));
        let event = DebugEvent::step_started(1, "step2", input.clone());

        if let DebugEvent::StepStarted {
            step_index,
            step_id,
            input: event_input,
        } = event
        {
            assert_eq!(step_index, 1);
            assert_eq!(step_id, "step2");
            assert_eq!(event_input, input);
        } else {
            panic!("Expected StepStarted event");
        }
    }

    #[test]
    fn test_debug_event_step_completed() {
        let result = FlowResult::Success(ValueRef::new(json!({"output": 42})));
        let event = DebugEvent::step_completed(2, "step3", result.clone());

        if let DebugEvent::StepCompleted {
            step_index,
            step_id,
            result: event_result,
        } = event
        {
            assert_eq!(step_index, 2);
            assert_eq!(step_id, "step3");
            assert_eq!(event_result, result);
        } else {
            panic!("Expected StepCompleted event");
        }
    }

    #[test]
    fn test_debug_event_accessors() {
        let event = DebugEvent::step_queued(5, "my_step");
        assert_eq!(event.step_index(), Some(5));
        assert_eq!(event.step_id(), Some("my_step"));

        let run_event = DebugEvent::run_completed(Uuid::nil(), true);
        assert_eq!(run_event.step_index(), None);
        assert_eq!(run_event.step_id(), None);
    }

    #[test]
    fn test_pending_action_serialization() {
        let action = PendingAction::execute_step(0, "step1");
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("execute_step"));

        let deserialized: PendingAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_pending_action_checks() {
        assert!(PendingAction::Complete.is_complete());
        assert!(!PendingAction::Complete.is_execute_step());
        assert!(!PendingAction::Complete.is_awaiting_input());

        assert!(PendingAction::execute_step(0, "s").is_execute_step());
        assert!(!PendingAction::execute_step(0, "s").is_complete());

        assert!(PendingAction::AwaitingInput.is_awaiting_input());
        assert!(!PendingAction::AwaitingInput.is_complete());
    }
}
