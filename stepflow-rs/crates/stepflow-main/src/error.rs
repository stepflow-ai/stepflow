// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

use std::borrow::Cow;
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
    #[error("Failed to initialize REPL")]
    ReplInit,
    #[error("REPL command error: {0}")]
    ReplCommand(String),
    #[error("Configuration error")]
    Configuration,
    #[error("Server error")]
    ServerError,
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Internal error with paths")]
    Path,
    #[error("Internal error: {0}")]
    Internal(Cow<'static, str>),
}

pub type Result<T, E = error_stack::Report<MainError>> = std::result::Result<T, E>;

impl MainError {
    pub fn internal(msg: impl Into<Cow<'static, str>>) -> Self {
        MainError::Internal(msg.into())
    }
}
