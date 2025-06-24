use stepflow_core::workflow::Component;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("error initializing plugin")]
    Initializing,
    #[error("error getting component info")]
    ComponentInfo,
    #[error("unknown component: {0}")]
    UnknownComponent(Component),
    #[error("error executing component")]
    Execution,
    #[error("error importing user-defined function")]
    UdfImport,
    #[error("error executing user-defined function")]
    UdfExecution,
    #[error("error decoding user-defined function")]
    UdfResults,
    #[error("no step plugin registered for protocol '{0}'")]
    UnknownScheme(String),
    #[error("unable to downcast plugin for protocol '{0}'")]
    DowncastErr(String),
    #[error("plugins already initialized")]
    AlreadyInitialized,
    #[error("invalid input")]
    InvalidInput,
    #[error("error creating plugin")]
    CreatePlugin,
    #[error("generic error: {0}")]
    Generic(String),
}

impl PluginError {
    pub fn new(message: impl Into<String>) -> Self {
        Self::Generic(message.into())
    }
}

pub type Result<T, E = error_stack::Report<PluginError>> = std::result::Result<T, E>;
