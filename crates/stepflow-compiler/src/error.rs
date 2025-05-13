use thiserror::Error;

#[derive(Error, Debug)]
pub enum CompileError {
    #[error("Duplicate step name '{0}'")]
    DuplicateStepId(String),
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

    #[error("missing step execution info")]
    MissingStepExecution,
}

pub type Result<T, E = error_stack::Report<CompileError>> = std::result::Result<T, E>;
