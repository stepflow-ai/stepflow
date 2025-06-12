/// Error types for workflow analysis
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("Malformed reference in workflow: {message}")]
    MalformedReference { message: String },
    #[error("Step not found: {step_id}")]
    StepNotFound { step_id: String },
}

pub type Result<T> = error_stack::Result<T, AnalysisError>;