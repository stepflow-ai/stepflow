use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
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
}

pub type Result<T, E = error_stack::Report<PluginError>> = std::result::Result<T, E>;
