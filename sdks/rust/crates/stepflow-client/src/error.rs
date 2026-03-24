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

/// Errors returned by [`crate::StepflowClient`] operations.
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("Failed to connect to orchestrator at {url}: {source}")]
    Connection {
        url: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Local orchestrator error: {0}")]
    LocalServer(String),
}

/// Errors returned by [`crate::FlowBuilder`].
#[derive(thiserror::Error, Debug)]
pub enum BuilderError {
    #[error("Output expression not set — call .output() before .build()")]
    OutputNotSet,
    #[error("Duplicate step ID: {0}")]
    DuplicateStep(String),
}

pub type ClientResult<T> = Result<T, ClientError>;
pub type BuilderResult<T> = Result<T, BuilderError>;
