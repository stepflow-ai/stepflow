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

use crate::Diagnostics;

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

/// Validation error with diagnostics (user errors)
///
/// This represents validation failures in the workflow structure,
/// such as undefined references, circular dependencies, etc.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub diagnostics: Diagnostics,
}

impl ValidationError {
    /// Create a ValidationError from diagnostics
    pub fn from_diagnostics(diagnostics: Diagnostics) -> Self {
        Self { diagnostics }
    }

    /// Check if there are any fatal errors
    pub fn has_fatal_errors(&self) -> bool {
        self.diagnostics.has_fatal()
    }

    /// Get a summary message of the validation errors
    pub fn message(&self) -> String {
        let (fatal, error, warning) = self.diagnostics.counts();
        format!(
            "Workflow validation failed with {} fatal, {} error, and {} warning diagnostics",
            fatal, error, warning
        )
    }

    /// Get the diagnostic counts (fatal, error, warning)
    pub fn counts(&self) -> (usize, usize, usize) {
        self.diagnostics.counts()
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for ValidationError {}
