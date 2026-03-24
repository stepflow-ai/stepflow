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

use stepflow_proto::TaskErrorCode;

/// Errors that a component implementation can return from [`crate::Component::execute`].
///
/// Each variant maps to a specific [`TaskErrorCode`] that is reported back to the orchestrator.
#[derive(thiserror::Error, Debug)]
pub enum ComponentError {
    /// The component received invalid or malformed input.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// The component encountered a runtime failure.
    #[error("Component failed: {0}")]
    Failed(String),

    /// A required resource (model, database, external service) is unavailable.
    #[error("Resource unavailable: {0}")]
    ResourceUnavailable(String),

    /// A generic worker-level error not covered by the above variants.
    #[error("Worker error: {0}")]
    WorkerError(String),
}

impl ComponentError {
    /// Map this error to the corresponding protobuf [`TaskErrorCode`].
    pub fn task_error_code(&self) -> TaskErrorCode {
        match self {
            ComponentError::InvalidInput(_) => TaskErrorCode::InvalidInput,
            ComponentError::Failed(_) => TaskErrorCode::ComponentFailed,
            ComponentError::ResourceUnavailable(_) => TaskErrorCode::ResourceUnavailable,
            ComponentError::WorkerError(_) => TaskErrorCode::WorkerError,
        }
    }
}

/// Errors returned by [`crate::ComponentContext`] operations.
#[derive(thiserror::Error, Debug)]
pub enum ContextError {
    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),
    #[error("Failed to connect: {0}")]
    Connection(#[from] tonic::transport::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Blob operation failed: {0}")]
    BlobFailed(String),
    #[error("Run failed: {0}")]
    RunFailed(String),
}

/// Errors returned by [`crate::Worker::run`].
#[derive(thiserror::Error, Debug)]
pub enum WorkerError {
    #[error("Failed to connect to tasks service at {url}: {source}")]
    Connection {
        url: String,
        #[source]
        source: tonic::transport::Error,
    },
    #[error("Tasks stream error: {0}")]
    Stream(tonic::Status),
    #[error("Max retries ({attempts}) exceeded connecting to tasks service")]
    MaxRetriesExceeded { attempts: u32 },
    #[error("Worker configuration error: {0}")]
    Config(String),
    #[error("Duplicate component path: {0}")]
    DuplicateComponent(String),
    #[error("Invalid component route pattern: {0}")]
    InvalidComponentRoute(String),
}
