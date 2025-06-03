#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Internal state store error")]
    Internal,

    #[error("Blob not found: {blob_id}")]
    BlobNotFound { blob_id: String },
}

pub type Result<T, E = error_stack::Report<StateError>> = std::result::Result<T, E>;
