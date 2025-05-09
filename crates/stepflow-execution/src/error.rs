use stepflow_workflow::{StepRef, ValueRef};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("undefined value {value_ref:?} for step '{}' output '{}'", step_ref.step, step_ref.output)]
    UndefinedValue {
        step_ref: StepRef,
        value_ref: ValueRef,
    },
    #[error("error executing plugin")]
    PluginError,
    #[error("plugin not found")]
    PluginNotFound,
    #[error("flow not compiled")]
    FlowNotCompiled,
}

pub type Result<T, E = error_stack::Report<ExecutionError>> = std::result::Result<T, E>;
