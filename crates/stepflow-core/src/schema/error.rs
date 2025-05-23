use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("expected simple object type, got {0}")]
    Unexpected(&'static str),
    #[error("missing object type")]
    MissingObject,
    #[error("invalid uses value: {0}")]
    InvalidUses(serde_json::Value),
    #[error("invalid schema")]
    InvalidSchema,
}

pub type Result<T, E = error_stack::Report<SchemaError>> = std::result::Result<T, E>;
