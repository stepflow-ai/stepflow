use stepflow_workflow::{BaseRef, Value};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("undefined value {0:?}")]
    UndefinedValue(BaseRef),
    #[error("undefined field {field:?} in {value:?}")]
    UndefinedField { field: String, value: Value },
    #[error("error executing plugin")]
    PluginError,
    #[error("plugin not found")]
    PluginNotFound,
    #[error("flow not compiled")]
    FlowNotCompiled,
}

pub type Result<T, E = error_stack::Report<ExecutionError>> = std::result::Result<T, E>;
