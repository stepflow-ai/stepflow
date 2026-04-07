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

pub mod blob;
pub mod blob_ref;
pub mod component;
pub mod environment;
pub mod error_code;
pub mod run_params;
pub mod status;
pub mod transport_retry;

// Re-export everything from stepflow-flow so that downstream crates
// continue to work with `stepflow_core::` paths unchanged.
pub use stepflow_flow::discriminator_schema;
pub use stepflow_flow::error_stack;
pub use stepflow_flow::json_schema;
pub use stepflow_flow::schema;
pub use stepflow_flow::values;
pub use stepflow_flow::workflow;

pub use stepflow_flow::ErrorStack;
pub use stepflow_flow::ErrorStackEntry;
pub use stepflow_flow::FlowError;
pub use stepflow_flow::FlowResult;
pub use stepflow_flow::TaskErrorCode;
pub use stepflow_flow::ValueExpr;

// Re-export commonly used types
pub use blob::{BlobData, BlobId, BlobMetadata, BlobType};
pub use blob_ref::BlobRef;
pub use environment::StepflowEnvironment;
pub use error_code::ErrorCode;
pub use run_params::{DEFAULT_WAIT_TIMEOUT_SECS, GetRunParams, ResultOrder, SubmitRunParams};
pub use transport_retry::RetryConfig;

// Re-export the proto TaskErrorCode for code that needs it directly.
pub use stepflow_proto::TaskErrorCode as ProtoTaskErrorCode;

/// Convert a proto [`ProtoTaskErrorCode`] to the flow-level [`TaskErrorCode`].
pub fn task_error_code_from_proto(proto: stepflow_proto::TaskErrorCode) -> TaskErrorCode {
    match proto {
        stepflow_proto::TaskErrorCode::Unspecified => TaskErrorCode::Unspecified,
        stepflow_proto::TaskErrorCode::Timeout => TaskErrorCode::Timeout,
        stepflow_proto::TaskErrorCode::InvalidInput => TaskErrorCode::InvalidInput,
        stepflow_proto::TaskErrorCode::ComponentFailed => TaskErrorCode::ComponentFailed,
        stepflow_proto::TaskErrorCode::Cancelled => TaskErrorCode::Cancelled,
        stepflow_proto::TaskErrorCode::Unreachable => TaskErrorCode::Unreachable,
        stepflow_proto::TaskErrorCode::ComponentNotFound => TaskErrorCode::ComponentNotFound,
        stepflow_proto::TaskErrorCode::ResourceUnavailable => TaskErrorCode::ResourceUnavailable,
        stepflow_proto::TaskErrorCode::ExpressionFailure => TaskErrorCode::ExpressionFailure,
        stepflow_proto::TaskErrorCode::OrchestratorError => TaskErrorCode::OrchestratorError,
        stepflow_proto::TaskErrorCode::WorkerError => TaskErrorCode::WorkerError,
    }
}

/// Convert a flow-level [`TaskErrorCode`] to the proto [`ProtoTaskErrorCode`].
pub fn task_error_code_to_proto(code: TaskErrorCode) -> stepflow_proto::TaskErrorCode {
    match code {
        TaskErrorCode::Unspecified => stepflow_proto::TaskErrorCode::Unspecified,
        TaskErrorCode::Timeout => stepflow_proto::TaskErrorCode::Timeout,
        TaskErrorCode::InvalidInput => stepflow_proto::TaskErrorCode::InvalidInput,
        TaskErrorCode::ComponentFailed => stepflow_proto::TaskErrorCode::ComponentFailed,
        TaskErrorCode::Cancelled => stepflow_proto::TaskErrorCode::Cancelled,
        TaskErrorCode::Unreachable => stepflow_proto::TaskErrorCode::Unreachable,
        TaskErrorCode::ComponentNotFound => stepflow_proto::TaskErrorCode::ComponentNotFound,
        TaskErrorCode::ResourceUnavailable => stepflow_proto::TaskErrorCode::ResourceUnavailable,
        TaskErrorCode::ExpressionFailure => stepflow_proto::TaskErrorCode::ExpressionFailure,
        TaskErrorCode::OrchestratorError => stepflow_proto::TaskErrorCode::OrchestratorError,
        TaskErrorCode::WorkerError => stepflow_proto::TaskErrorCode::WorkerError,
    }
}
