use stepflow_workflow::{StepOutput, StepRef, ValueRef};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompileError {
    #[error("Duplicate step name '{0}'")]
    DuplicateStepId(String),
    #[error("Invalid reference to output {0}")]
    InvalidValueRef(ValueRef),
    #[error("No plugin found")]
    NoPluginFound,
    #[error("Failed to get info for component")]
    ComponentInfo,
    #[error("steps[{step}].always_execute={actual} but plugin info says {expected}")]
    AlwaysExecuteDisagreement {
        step: usize,
        expected: bool,
        actual: bool,
    },
    #[error("steps[{step}].outputs={actual:?} but plugin info says {expected:?}")]
    OutputDisagreement {
        step: usize,
        expected: Vec<StepOutput>,
        actual: Vec<StepOutput>,
    },
    #[error("missing step execution info")]
    MissingStepExecution,
    #[error("no value ref for input {0}")]
    InvalidInputRef(String),
    #[error("no value ref for step '{}' output '{}'", .0.step, .0.output)]
    InvalidStepRef(StepRef),
}

pub type Result<T, E = error_stack::Report<CompileError>> = std::result::Result<T, E>;
