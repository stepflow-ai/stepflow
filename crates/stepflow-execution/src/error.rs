use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("slot {slot} out of bounds")]
    SlotOutOfBounds { slot: u32 },
    #[error("error executing plugin for '{0}'")]
    PluginError(Url),
    #[error("flow not compiled")]
    FlowNotCompiled,
}

pub type Result<T, E = error_stack::Report<ExecutionError>> = std::result::Result<T, E>;
