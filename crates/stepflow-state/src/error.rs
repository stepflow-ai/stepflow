#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Internal state store error")]
    Internal,

    #[error("Blob not found: {blob_id}")]
    BlobNotFound { blob_id: String },

    #[error("Step result not found for execution {execution_id}, step index {step_idx}")]
    StepResultNotFoundByIndex {
        execution_id: String,
        step_idx: usize,
    },

    #[error("Step result not found for execution {execution_id}, step id '{step_id}'")]
    StepResultNotFoundById {
        execution_id: String,
        step_id: String,
    },
}

pub type Result<T, E = error_stack::Report<StateError>> = std::result::Result<T, E>;
