use stepflow_workflow::{StepOutput, StepRef};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompileError {
    #[error("Duplicate step name '{0}'")]
    DuplicateStepId(String),
    #[error("Invalid reference to output '{}' from step '{}'", .0.step_id, .0.output)]
    InvalidStepRef(StepRef),
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
    #[error("invalid compilation: {0}")]
    Validation(String),
}

pub type Result<T, E = error_stack::Report<CompileError>> = std::result::Result<T, E>;
