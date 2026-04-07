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

//! Error code enum for task/step failures.
//!
//! This mirrors the `TaskErrorCode` proto enum but is defined as a native Rust type
//! so that `stepflow-flow` does not depend on protobuf code generation.

/// Categorizes the type of error that occurred during task execution.
///
/// Each variant maps to a specific retry policy:
/// - **Always retried (transport)**: [`Unreachable`](Self::Unreachable), [`Timeout`](Self::Timeout)
/// - **Retried with onError policy**: [`ComponentFailed`](Self::ComponentFailed),
///   [`ResourceUnavailable`](Self::ResourceUnavailable)
/// - **Never retried**: all others
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskErrorCode {
    /// Default value; should not be used explicitly.
    Unspecified,
    /// The task exceeded its execution deadline or heartbeat timeout.
    Timeout,
    /// The component rejected its input.
    InvalidInput,
    /// The component executed but returned a business-logic failure.
    ComponentFailed,
    /// The task was explicitly cancelled by the orchestrator.
    Cancelled,
    /// The worker or component could not be reached.
    Unreachable,
    /// The requested component does not exist on the worker.
    ComponentNotFound,
    /// A resource required by the component was not available.
    ResourceUnavailable,
    /// The orchestrator failed to resolve a value expression.
    ExpressionFailure,
    /// Catch-all for unexpected orchestrator errors.
    OrchestratorError,
    /// Catch-all for unexpected worker/SDK errors.
    WorkerError,
}

impl TaskErrorCode {
    /// Returns the proto-style name (e.g. `"TASK_ERROR_CODE_TIMEOUT"`).
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Unspecified => "TASK_ERROR_CODE_UNSPECIFIED",
            Self::Timeout => "TASK_ERROR_CODE_TIMEOUT",
            Self::InvalidInput => "TASK_ERROR_CODE_INVALID_INPUT",
            Self::ComponentFailed => "TASK_ERROR_CODE_COMPONENT_FAILED",
            Self::Cancelled => "TASK_ERROR_CODE_CANCELLED",
            Self::Unreachable => "TASK_ERROR_CODE_UNREACHABLE",
            Self::ComponentNotFound => "TASK_ERROR_CODE_COMPONENT_NOT_FOUND",
            Self::ResourceUnavailable => "TASK_ERROR_CODE_RESOURCE_UNAVAILABLE",
            Self::ExpressionFailure => "TASK_ERROR_CODE_EXPRESSION_FAILURE",
            Self::OrchestratorError => "TASK_ERROR_CODE_ORCHESTRATOR_ERROR",
            Self::WorkerError => "TASK_ERROR_CODE_WORKER_ERROR",
        }
    }

    /// Returns the numeric proto value.
    pub fn proto_number(&self) -> i32 {
        match self {
            Self::Unspecified => 0,
            Self::Timeout => 1,
            Self::InvalidInput => 2,
            Self::ComponentFailed => 3,
            Self::Cancelled => 4,
            Self::Unreachable => 5,
            Self::ComponentNotFound => 6,
            Self::ResourceUnavailable => 7,
            Self::ExpressionFailure => 8,
            Self::OrchestratorError => 9,
            Self::WorkerError => 10,
        }
    }
}

impl From<TaskErrorCode> for i32 {
    fn from(code: TaskErrorCode) -> i32 {
        code.proto_number()
    }
}

impl TryFrom<i32> for TaskErrorCode {
    type Error = i32;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unspecified),
            1 => Ok(Self::Timeout),
            2 => Ok(Self::InvalidInput),
            3 => Ok(Self::ComponentFailed),
            4 => Ok(Self::Cancelled),
            5 => Ok(Self::Unreachable),
            6 => Ok(Self::ComponentNotFound),
            7 => Ok(Self::ResourceUnavailable),
            8 => Ok(Self::ExpressionFailure),
            9 => Ok(Self::OrchestratorError),
            10 => Ok(Self::WorkerError),
            other => Err(other),
        }
    }
}

impl std::fmt::Display for TaskErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Strip the TASK_ERROR_CODE_ prefix for display
        let proto_name = self.as_str_name();
        let short = &proto_name["TASK_ERROR_CODE_".len()..];
        f.write_str(short)
    }
}
