use stepflow_workflow::Component;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("slot {slot} out of bounds")]
    SlotOutOfBounds { slot: u32 },
    #[error("error executing plugin")]
    PluginError,
    #[error("plugin not found")]
    PluginNotFound,
    #[error("flow not compiled")]
    FlowNotCompiled,
}

pub type Result<T, E = error_stack::Report<ExecutionError>> = std::result::Result<T, E>;
