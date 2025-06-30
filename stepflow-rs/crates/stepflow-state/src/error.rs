use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("State store initialization error")]
    Initialization,

    #[error("State store connection error")]
    Connection,

    #[error("Internal state store error")]
    Internal,

    #[error("Blob not found: {blob_id}")]
    BlobNotFound { blob_id: String },

    #[error("Step result not found for run {run_id}, step index {step_idx}")]
    StepResultNotFoundByIndex { run_id: String, step_idx: usize },

    #[error("Step result not found for run {run_id}, step id '{step_id}'")]
    StepResultNotFoundById { run_id: Uuid, step_id: String },

    #[error("Workflow not found: {workflow_hash}")]
    WorkflowNotFound { workflow_hash: String },

    #[error("Run not found: {run_id}")]
    RunNotFound { run_id: Uuid },

    #[error("Serialization error")]
    Serialization,
}

pub type Result<T, E = error_stack::Report<StateError>> = std::result::Result<T, E>;
