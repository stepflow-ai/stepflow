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
    #[error("flow not compiled")]
    FlowNotCompiled,
    #[error("error receiving input")]
    RecvInput,
    #[error("error recording result for step '{0}'")]
    RecordResult(String),
    #[error("internal error")]
    Internal,
    #[error("step panic")]
    StepPanic,
    #[error("step {step:?} failed")]
    StepFailed { step: String },
    #[error("step {step:?} error")]
    StepError { step: String },
    #[error("blob not found: {blob_id}")]
    BlobNotFound { blob_id: String },
    #[error("no plugin registered for protocol: {0}")]
    UnregisteredProtocol(String),
    #[error("workflow deadlock: no runnable steps and output cannot be resolved")]
    Deadlock,
    #[error("malformed reference: {message}")]
    MalformedReference { message: String },
    #[error("step not found: {step}")]
    StepNotFound { step: String },
    #[error("step not runnable: {step}")]
    StepNotRunnable { step: String },
    #[error("error accessing state store")]
    StateError,
    #[error("workflow analysis error: {message}")]
    AnalysisError { message: String },
}

pub type Result<T, E = error_stack::Report<ExecutionError>> = std::result::Result<T, E>;
