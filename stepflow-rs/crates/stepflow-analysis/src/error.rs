/// Error types for workflow analysis (internal/unexpected errors only)
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("Internal analysis error: {message}")]
    Internal { message: String },
    #[error("Malformed reference in workflow: {message}")]
    MalformedReference { message: String },
    #[error("Step not found: {step_id}")]
    StepNotFound { step_id: String },
    #[error("Error getting dependencies")]
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
