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

//! Observability infrastructure for Stepflow
//!
//! This crate provides unified logging and distributed tracing capabilities:
//! - Logging via the `log` crate with automatic trace context injection
//! - Distributed tracing via `fastrace` with OpenTelemetry export
//!
//! # Key Features
//!
//! - **Automatic trace context**: All logs automatically include `trace_id` and `span_id`
//! - **Zero-cost tracing**: When disabled, tracing has no runtime overhead
//! - **OpenTelemetry support**: Export traces and logs to any OTLP-compatible backend
//! - **Multiple formats**: JSON or plain text logging

pub use fastrace;
pub use log;

mod run_diagnostic_context;
pub use run_diagnostic_context::{RunDiagnostic, RunIdGuard, StepIdGuard, get_run_id, get_step_id};

/// Binary-specific observability configuration
#[derive(Clone, Debug, Default)]
pub struct BinaryObservabilityConfig {
    /// Include run diagnostic (run_id, step_id) in logs
    ///
    /// Set to true for binaries that execute workflows (stepflow-server, CLI run command)
    /// Set to false for binaries that don't execute workflows (load balancer, CLI submit command)
    pub include_run_diagnostic: bool,
}

/// Configuration for observability (logging + tracing)
#[derive(Clone, Debug)]
pub struct ObservabilityConfig {
    /// Log level filter
    pub log_level: log::LevelFilter,

    /// Log output format
    pub log_format: LogFormat,

    /// Log output destination (exclusive - only one destination at a time)
    pub log_destination: LogDestination,

    /// Enable distributed tracing
    pub trace_enabled: bool,

    /// Binary-specific configuration
    pub binary_config: BinaryObservabilityConfig,

    /// OTLP endpoint (e.g., "http://localhost:4317")
    /// Used for both trace and log export when log_destination is LogDestination::OpenTelemetry
    pub otlp_endpoint: Option<String>,

    /// Service name for traces/logs
    pub service_name: String,
}

/// Log output format
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFormat {
    /// Structured JSON logs
    Json,
    /// Human-readable text logs
    Text,
}

/// Log output destination (exclusive - only one at a time)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogDestination {
    /// Log to stdout
    Stdout,
    /// Log to OpenTelemetry (uses otlp_endpoint from config)
    OpenTelemetry,
    /// Log to a file (appends)
    File(std::path::PathBuf),
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_level: log::LevelFilter::Info,
            log_format: LogFormat::Text,
            log_destination: LogDestination::Stdout,
            trace_enabled: false,
            binary_config: BinaryObservabilityConfig::default(),
            otlp_endpoint: None,
            service_name: "stepflow".to_string(),
        }
    }
}

/// Initialize observability (logging + tracing)
///
/// Returns a guard that must be held for the lifetime of the application.
/// When the guard is dropped, tracing will be flushed and shut down.
///
/// # Example
///
/// ```no_run
/// use stepflow_observability::{
///     ObservabilityConfig, BinaryObservabilityConfig, LogFormat, LogDestination, init_observability
/// };
///
/// let config = ObservabilityConfig {
///     log_level: log::LevelFilter::Info,
///     log_format: LogFormat::Json,
///     log_destination: LogDestination::Stdout,
///     trace_enabled: true,
///     binary_config: BinaryObservabilityConfig {
///         include_run_diagnostic: true,  // Enable for workflow execution binaries
///     },
///     otlp_endpoint: None,
///     service_name: "my-service".to_string(),
/// };
///
/// let _guard = init_observability(config).unwrap();
/// log::info!("Service started");
/// ```
pub fn init_observability(config: ObservabilityConfig) -> Result<ObservabilityGuard> {
    // Initialize tracing first (so logger can access trace context)
    let trace_guard = if config.trace_enabled {
        Some(init_tracing(&config)?)
    } else {
        None
    };

    // Initialize logging with trace context integration
    let log_guard = init_logging(&config)?;

    Ok(ObservabilityGuard {
        _log_guard: log_guard,
        _trace_guard: trace_guard,
    })
}

fn init_logging(config: &ObservabilityConfig) -> Result<LogGuard> {
    use logforth::append;
    use logforth::diagnostic::FastraceDiagnostic;
    use logforth::layout::JsonLayout;
    use logforth::layout::TextLayout;
    use logforth::record::LevelFilter;

    // Convert log::LevelFilter to logforth::record::LevelFilter
    let level_filter = match config.log_level {
        log::LevelFilter::Off => LevelFilter::Off,
        log::LevelFilter::Error => LevelFilter::Error,
        log::LevelFilter::Warn => LevelFilter::Warn,
        log::LevelFilter::Info => LevelFilter::Info,
        log::LevelFilter::Debug => LevelFilter::Debug,
        log::LevelFilter::Trace => LevelFilter::Trace,
    };

    let builder = logforth::starter_log::builder();

    // Configure single exclusive destination with fastrace diagnostic (always)
    // and optionally run diagnostic (if binary_config.include_run_diagnostic is true)
    let builder = match (
        &config.log_destination,
        config.log_format,
        config.binary_config.include_run_diagnostic,
    ) {
        (LogDestination::Stdout, LogFormat::Json, true) => builder.dispatch(|d| {
            d.filter(level_filter)
                .diagnostic(FastraceDiagnostic::default())
                .diagnostic(RunDiagnostic)
                .append(append::Stdout::default().with_layout(JsonLayout::default()))
        }),
        (LogDestination::Stdout, LogFormat::Json, false) => builder.dispatch(|d| {
            d.filter(level_filter)
                .diagnostic(FastraceDiagnostic::default())
                .append(append::Stdout::default().with_layout(JsonLayout::default()))
        }),
        (LogDestination::Stdout, LogFormat::Text, true) => builder.dispatch(|d| {
            d.filter(level_filter)
                .diagnostic(FastraceDiagnostic::default())
                .diagnostic(RunDiagnostic)
                .append(append::Stdout::default().with_layout(TextLayout::default()))
        }),
        (LogDestination::Stdout, LogFormat::Text, false) => builder.dispatch(|d| {
            d.filter(level_filter)
                .diagnostic(FastraceDiagnostic::default())
                .append(append::Stdout::default().with_layout(TextLayout::default()))
        }),
        (LogDestination::File(path), LogFormat::Json, true) => {
            // Special case: /dev/null means discard logs (used in tests)
            if path == std::path::Path::new("/dev/null") {
                return Ok(LogGuard);
            }
            let file_append = append::file::FileBuilder::new(path.clone(), "app_log")
                .layout(JsonLayout::default())
                .build()
                .map_err(|e| {
                    error_stack::Report::new(ObservabilityError::LogInitError)
                        .attach_printable(format!("Failed to create file appender for {:?}", path))
                        .attach_printable(format!("{:?}", e))
                })?;
            builder.dispatch(|d| {
                d.filter(level_filter)
                    .diagnostic(FastraceDiagnostic::default())
                    .diagnostic(RunDiagnostic)
                    .append(file_append)
            })
        }
        (LogDestination::File(path), LogFormat::Json, false) => {
            // Special case: /dev/null means discard logs (used in tests)
            if path == std::path::Path::new("/dev/null") {
                return Ok(LogGuard);
            }
            let file_append = append::file::FileBuilder::new(path.clone(), "app_log")
                .layout(JsonLayout::default())
                .build()
                .map_err(|e| {
                    error_stack::Report::new(ObservabilityError::LogInitError)
                        .attach_printable(format!("Failed to create file appender for {:?}", path))
                        .attach_printable(format!("{:?}", e))
                })?;
            builder.dispatch(|d| {
                d.filter(level_filter)
                    .diagnostic(FastraceDiagnostic::default())
                    .append(file_append)
            })
        }
        (LogDestination::File(path), LogFormat::Text, true) => {
            // Special case: /dev/null means discard logs (used in tests)
            if path == std::path::Path::new("/dev/null") {
                return Ok(LogGuard);
            }
            let file_append = append::file::FileBuilder::new(path.clone(), "app_log")
                .layout(TextLayout::default())
                .build()
                .map_err(|e| {
                    error_stack::Report::new(ObservabilityError::LogInitError)
                        .attach_printable(format!("Failed to create file appender for {:?}", path))
                        .attach_printable(format!("{:?}", e))
                })?;
            builder.dispatch(|d| {
                d.filter(level_filter)
                    .diagnostic(FastraceDiagnostic::default())
                    .diagnostic(RunDiagnostic)
                    .append(file_append)
            })
        }
        (LogDestination::File(path), LogFormat::Text, false) => {
            // Special case: /dev/null means discard logs (used in tests)
            if path == std::path::Path::new("/dev/null") {
                return Ok(LogGuard);
            }
            let file_append = append::file::FileBuilder::new(path.clone(), "app_log")
                .layout(TextLayout::default())
                .build()
                .map_err(|e| {
                    error_stack::Report::new(ObservabilityError::LogInitError)
                        .attach_printable(format!("Failed to create file appender for {:?}", path))
                        .attach_printable(format!("{:?}", e))
                })?;
            builder.dispatch(|d| {
                d.filter(level_filter)
                    .diagnostic(FastraceDiagnostic::default())
                    .append(file_append)
            })
        }
        (LogDestination::OpenTelemetry, _, true) => {
            // TODO: Implement OpenTelemetry log appender
            // For now, fall back to stdout
            eprintln!(
                "WARNING: OpenTelemetry log output not yet implemented, falling back to stdout"
            );
            builder.dispatch(|d| {
                d.filter(level_filter)
                    .diagnostic(FastraceDiagnostic::default())
                    .diagnostic(RunDiagnostic)
                    .append(append::Stdout::default().with_layout(JsonLayout::default()))
            })
        }
        (LogDestination::OpenTelemetry, _, false) => {
            // TODO: Implement OpenTelemetry log appender
            // For now, fall back to stdout
            eprintln!(
                "WARNING: OpenTelemetry log output not yet implemented, falling back to stdout"
            );
            builder.dispatch(|d| {
                d.filter(level_filter)
                    .diagnostic(FastraceDiagnostic::default())
                    .append(append::Stdout::default().with_layout(JsonLayout::default()))
            })
        }
    };

    builder.apply();

    Ok(LogGuard)
}

fn init_tracing(config: &ObservabilityConfig) -> Result<TraceGuard> {
    if let Some(_endpoint) = &config.otlp_endpoint {
        // TODO: Export traces to OTLP
        // For now, use console reporter
        fastrace::set_reporter(
            fastrace::collector::ConsoleReporter,
            fastrace::collector::Config::default(),
        );
    } else {
        // Console reporter for development
        fastrace::set_reporter(
            fastrace::collector::ConsoleReporter,
            fastrace::collector::Config::default(),
        );
    }

    Ok(TraceGuard)
}

/// Guard that ensures proper cleanup of observability systems
pub struct ObservabilityGuard {
    _log_guard: LogGuard,
    _trace_guard: Option<TraceGuard>,
}

struct LogGuard;
struct TraceGuard;

impl Drop for TraceGuard {
    fn drop(&mut self) {
        fastrace::flush();
    }
}

/// Errors that can occur during observability initialization
#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error("Failed to initialize logging")]
    LogInitError,
    #[error("Failed to initialize tracing")]
    TraceInitError,
    #[error("Failed to initialize OTLP")]
    OtlpInitError,
}

pub type Result<T> = std::result::Result<T, error_stack::Report<ObservabilityError>>;
