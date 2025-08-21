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

/// Error types for workflow analysis (internal/unexpected errors only)
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("Internal analysis error: {message}")]
    Internal { message: String },
    #[error("Malformed reference in workflow: {message}")]
    MalformedReference { message: String },
    #[error("Step not found: {step_id}")]
    StepNotFound { step_id: String },
    #[error("Error extracting dependencies")]
    DependencyAnalysis,
}

impl AnalysisError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

pub type Result<T> = error_stack::Result<T, AnalysisError>;
