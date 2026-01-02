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

use stepflow_core::BlobId;
use stepflow_core::workflow::ValueRef;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug, PartialEq)]
pub enum ExecutionError {
    #[error("undefined step reference: {0}")]
    UndefinedValue(String),
    #[error("undefined field {field:?} in {value:?}")]
    UndefinedField { field: String, value: ValueRef },
    #[error("error executing plugin")]
    PluginError,
    #[error("error initializing plugin")]
    PluginInitialization,
    #[error("flow not compiled")]
    FlowNotCompiled,
    #[error("error receiving input")]
    RecvInput,
    #[error("error recording result for step '{0}'")]
    RecordResult(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("step panic")]
    StepPanic,
    #[error("step {step:?} failed")]
    StepFailed { step: String },
    #[error("blob not found: {blob_id}")]
    BlobNotFound { blob_id: String },
    #[error("no plugin registered for protocol: {0}")]
    UnregisteredProtocol(String),
    #[error("error routing component")]
    RouterError,
    #[error("workflow deadlock: no runnable steps and output cannot be resolved")]
    Deadlock,
    #[error("malformed reference: {message}")]
    MalformedReference { message: String },
    #[error("step not found: {step}")]
    StepNotFound { step: String },
    #[error("step not completed: {step}")]
    StepNotCompleted { step: String },
    #[error("step not runnable: {step}")]
    StepNotRunnable { step: String },
    #[error("error accessing state store")]
    StateError,
    #[error("error analyzing workflow")]
    AnalysisError,
    #[error("execution '{0}' not found")]
    ExecutionNotFound(Uuid),
    #[error("workflow '{0}' not found")]
    WorkflowNotFound(BlobId),
    #[error("failed to resolve input for step '{0}'")]
    ResolveStepInput(String),
    #[error("failed to resolve value for step '{0}'")]
    ResolveStepOutput(String),
    #[error("failed to resolve workflow output")]
    ResolveWorkflowOutput,
    #[error("failed to apply workflow overrides")]
    OverrideError,
}

impl ExecutionError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

pub type Result<T, E = error_stack::Report<ExecutionError>> = std::result::Result<T, E>;
