use stepflow_core::workflow::{BaseRef, ValueRef};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ExecutionError {
    #[error("undefined value {0:?}")]
    UndefinedValue(BaseRef),
    #[error("undefined field {field:?} in {value:?}")]
    UndefinedField { field: String, value: ValueRef },
    #[error("error executing plugin")]
    PluginError,
    #[error("plugin not found")]
    PluginNotFound,
    #[error("flow not compiled")]
    FlowNotCompiled,
    #[error("error receiving input")]
    RecvInput,
    #[error("error recording result")]
    RecordResult,
    #[error("internal error")]
    Internal,
    #[error("step panic")]
    StepPanic,
    #[error("step {step:?} failed")]
    StepFailed { step: String },
    #[error("blob not found: {blob_id}")]
    BlobNotFound { blob_id: String },
}

pub type Result<T, E = error_stack::Report<ExecutionError>> = std::result::Result<T, E>;
