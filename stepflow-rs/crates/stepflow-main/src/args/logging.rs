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

use error_stack::ResultExt as _;
use std::{fs::File, path::Path};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use crate::{MainError, Result};

/// Log level for tracing configuration
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        };
        write!(f, "{s}")
    }
}

/// Initialize tracing with the specified configuration.
pub fn init_tracing(
    log_level: &LogLevel,
    other_log_level: &LogLevel,
    log_file: Option<&Path>,
) -> Result<()> {
    let filter_str = format!("stepflow_={log_level},{other_log_level}");
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter_str));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339());

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_error::ErrorLayer::default());

    match log_file {
        Some(file_path) => {
            let file = File::create(file_path)
                .change_context_lazy(|| MainError::CreateOutput(file_path.to_owned()))?;
            let fmt_layer = fmt_layer.with_writer(file);
            registry
                .with(fmt_layer)
                .try_init()
                .map_err(|_| MainError::TracingInit)?;
        }
        None => {
            registry
                .with(fmt_layer)
                .try_init()
                .map_err(|_| MainError::TracingInit)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Trace.to_string(), "trace");
        assert_eq!(LogLevel::Debug.to_string(), "debug");
        assert_eq!(LogLevel::Info.to_string(), "info");
        assert_eq!(LogLevel::Warn.to_string(), "warn");
        assert_eq!(LogLevel::Error.to_string(), "error");
    }
}
