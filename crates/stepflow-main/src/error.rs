use std::path::PathBuf;

#[derive(Debug, thiserror::Error, Clone)]
pub enum MainError {
    #[error("Missing file: {}", .0.display())]
    MissingFile(PathBuf),
    #[error("Invalid file: {}", .0.display())]
    InvalidFile(PathBuf),
    #[error("Unrecognized file extension: {}", .0.display())]
    UnrecognizedFileExtension(PathBuf),
    #[error("Failed to create output file: {}", .0.display())]
    CreateOutput(PathBuf),
    #[error("Failed to write output file: {}", .0.display())]
    WriteOutput(PathBuf),
    #[error("Unable to locate command: {0:?}")]
    MissingCommand(String),
    #[error("Failed to register plugin")]
    RegisterPlugin,
    #[error("Failed to execute flow")]
    FlowExecution,
    #[error("Failed to initialize plugins")]
    InitializePlugins,
    #[error("Multiple stepflow config files found in directory: {0:?}")]
    MultipleStepflowConfigs(PathBuf),
    #[error("Stepflow config not found")]
    StepflowConfigNotFound,
    #[error("Plugin communication failed")]
    PluginCommunication,
    #[error("Serialization failed")]
    SerializationError,
    #[error("Failed to initialize tracing")]
    TracingInit,
}

pub type Result<T, E = error_stack::Report<MainError>> = std::result::Result<T, E>;
