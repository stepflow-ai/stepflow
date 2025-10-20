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

use opentelemetry_otlp::WithExportConfig;

mod run_diagnostic_context;
pub use run_diagnostic_context::{RunDiagnostic, RunIdGuard, StepIdGuard, get_run_id, get_step_id};

/// Binary-specific observability configuration
///
/// This configuration is not exposed as CLI arguments and should be set programmatically
/// based on the binary type.
#[derive(Clone, Debug)]
pub struct BinaryObservabilityConfig {
    /// Service name for traces/logs
    pub service_name: &'static str,

    /// Include run diagnostic (run_id, step_id) in logs
    ///
    /// Set to true for binaries that execute workflows (stepflow-server, CLI run command)
    /// Set to false for binaries that don't execute workflows (load balancer, CLI submit command)
    pub include_run_diagnostic: bool,
}

/// Configuration for observability (logging + tracing)
#[derive(Debug, clap::Args)]
pub struct ObservabilityConfig {
    /// Log level filter for stepflow crates
    #[arg(long, default_value = "info", env = "STEPFLOW_LOG_LEVEL", value_parser = parse_log_level)]
    pub log_level: log::LevelFilter,

    /// Log level filter for non-stepflow crates (e.g., dependencies)
    /// When None, uses the same level as log_level
    #[arg(long, env = "STEPFLOW_OTHER_LOG_LEVEL", value_parser = parse_log_level)]
    pub other_log_level: Option<log::LevelFilter>,

    /// Log output format
    #[arg(long, default_value = "text", env = "STEPFLOW_LOG_FORMAT")]
    pub log_format: LogFormat,

    /// Log to file (instead of stdout)
    #[arg(long, env = "STEPFLOW_LOG_FILE")]
    pub log_file: Option<std::path::PathBuf>,

    /// Enable distributed tracing
    #[arg(long, env = "STEPFLOW_TRACE_ENABLED")]
    pub trace_enabled: bool,

    /// OTLP endpoint (e.g., "http://localhost:4317")
    /// Used for both trace and log export when log_destination is LogDestination::OpenTelemetry
    #[arg(long, env = "STEPFLOW_OTLP_ENDPOINT")]
    pub otlp_endpoint: Option<String>,
}

impl ObservabilityConfig {
    /// Get the effective log destination based on the configuration
    pub fn log_destination(&self) -> LogDestination<'_> {
        match &self.log_file {
            Some(path) => LogDestination::File(path),
            None => LogDestination::Stdout,
        }
    }
}

fn parse_log_level(s: &str) -> std::result::Result<log::LevelFilter, String> {
    match s.to_lowercase().as_str() {
        "off" => Ok(log::LevelFilter::Off),
        "error" => Ok(log::LevelFilter::Error),
        "warn" => Ok(log::LevelFilter::Warn),
        "info" => Ok(log::LevelFilter::Info),
        "debug" => Ok(log::LevelFilter::Debug),
        "trace" => Ok(log::LevelFilter::Trace),
        _ => Err(format!("Invalid log level: {}", s)),
    }
}

/// Log output format
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum LogFormat {
    /// Structured JSON logs
    Json,
    /// Human-readable text logs
    Text,
}

/// Log output destination (computed from configuration)
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum LogDestination<'a> {
    /// Log to stdout
    Stdout,
    /// Log to OpenTelemetry (uses otlp_endpoint from config)
    OpenTelemetry,
    /// Log to a file (appends)
    File(&'a std::path::Path),
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
///     other_log_level: Some(log::LevelFilter::Warn),  // Separate level for dependencies
///     log_format: LogFormat::Json,
///     log_file: None,
///     trace_enabled: true,
///     otlp_endpoint: None,
/// };
///
/// let binary_config = BinaryObservabilityConfig {
///     service_name: "my-service",
///     include_run_diagnostic: true,  // Enable for workflow execution binaries
/// };
///
/// let _guard = init_observability(&config, binary_config).unwrap();
/// log::info!("Service started");
/// ```
pub fn init_observability(
    config: &ObservabilityConfig,
    binary_config: BinaryObservabilityConfig,
) -> Result<ObservabilityGuard> {
    // Initialize tracing first (so logger can access trace context)
    let trace_guard = if config.trace_enabled {
        Some(init_tracing(config, &binary_config)?)
    } else {
        None
    };

    // Initialize logging with trace context integration
    let log_guard = init_logging(config, &binary_config)?;

    Ok(ObservabilityGuard {
        _log_guard: log_guard,
        _trace_guard: trace_guard,
    })
}

fn init_logging(
    config: &ObservabilityConfig,
    binary_config: &BinaryObservabilityConfig,
) -> Result<LogGuard> {
    use logforth::diagnostic::FastraceDiagnostic;
    use logforth::filter::env_filter::EnvFilterBuilder;

    // Helper to convert log level to string for EnvFilter
    fn level_to_str(level: log::LevelFilter) -> &'static str {
        match level {
            log::LevelFilter::Off => "off",
            log::LevelFilter::Error => "error",
            log::LevelFilter::Warn => "warn",
            log::LevelFilter::Info => "info",
            log::LevelFilter::Debug => "debug",
            log::LevelFilter::Trace => "trace",
        }
    }

    // Special case: /dev/null means discard logs (used in tests)
    if let LogDestination::File(path) = config.log_destination()
        && path == std::path::Path::new("/dev/null")
    {
        return Ok(LogGuard);
    }

    let mut builder = logforth::starter_log::builder();

    // When other_log_level is specified, we need TWO separate dispatch chains with EnvFilter
    // When other_log_level is None, we only need ONE dispatch chain
    match config.other_log_level {
        Some(other_level) => {
            // Dual-level logging: separate dispatch for stepflow vs non-stepflow crates

            // Build EnvFilter spec for stepflow crates: "stepflow_=<level>"
            let stepflow_spec = format!("stepflow_={}", level_to_str(config.log_level));
            let stepflow_filter = EnvFilterBuilder::from_spec(&stepflow_spec).build();

            let destination = config.log_destination();
            let format = config.log_format;

            // Dispatch 1: stepflow_* crates at the main log level
            builder = builder.dispatch(|d| {
                let mut d = d
                    .filter(stepflow_filter)
                    .diagnostic(FastraceDiagnostic::default());

                if binary_config.include_run_diagnostic {
                    d = d.diagnostic(RunDiagnostic);
                }

                d.append(create_appender(destination, format))
            });

            // Build EnvFilter spec for everything except stepflow: "<level>,stepflow_=off"
            // This sets a global level and then disables stepflow crates
            let other_spec = format!("{},stepflow_=off", level_to_str(other_level));
            let other_filter = EnvFilterBuilder::from_spec(&other_spec).build();

            // Dispatch 2: non-stepflow crates at the other log level
            builder = builder.dispatch(|d| {
                let mut d = d
                    .filter(other_filter)
                    .diagnostic(FastraceDiagnostic::default());

                if binary_config.include_run_diagnostic {
                    d = d.diagnostic(RunDiagnostic);
                }

                d.append(create_appender(destination, format))
            });
        }
        None => {
            // Single-level logging: one dispatch chain for all crates with a global level filter
            let filter_spec = level_to_str(config.log_level);
            let filter = EnvFilterBuilder::from_spec(filter_spec).build();

            let destination = config.log_destination();
            let format = config.log_format;

            builder = builder.dispatch(|d| {
                let mut d = d.filter(filter).diagnostic(FastraceDiagnostic::default());

                if binary_config.include_run_diagnostic {
                    d = d.diagnostic(RunDiagnostic);
                }

                d.append(create_appender(destination, format))
            });
        }
    }

    builder.apply();

    Ok(LogGuard)
}

/// Create an appender based on configuration
fn create_appender(destination: LogDestination, format: LogFormat) -> Box<dyn logforth::Append> {
    use logforth::append;
    use logforth::layout::JsonLayout;
    use logforth::layout::TextLayout;

    match (destination, format) {
        (LogDestination::Stdout, LogFormat::Json) => {
            Box::new(append::Stdout::default().with_layout(JsonLayout::default()))
        }
        (LogDestination::Stdout, LogFormat::Text) => {
            Box::new(append::Stdout::default().with_layout(TextLayout::default()))
        }
        (LogDestination::File(path), LogFormat::Json) => {
            // Note: Error handling moved to init_logging (checked before dispatch)
            let file_appender = append::file::FileBuilder::new(path.to_path_buf(), "app_log")
                .layout(JsonLayout::default())
                .build()
                .expect("File appender creation should have been validated");
            Box::new(file_appender)
        }
        (LogDestination::File(path), LogFormat::Text) => {
            let file_appender = append::file::FileBuilder::new(path.to_path_buf(), "app_log")
                .layout(TextLayout::default())
                .build()
                .expect("File appender creation should have been validated");
            Box::new(file_appender)
        }
        (LogDestination::OpenTelemetry, _) => {
            // TODO(#388): Implement OpenTelemetry log appender via logforth
            // Logforth doesn't yet have built-in OTLP appender.
            // For now, fall back to stdout with JSON layout.
            // Alternative: Use opentelemetry-appender-log directly (requires different setup)
            Box::new(append::Stdout::default().with_layout(JsonLayout::default()))
        }
    }
}

fn init_tracing(
    config: &ObservabilityConfig,
    binary_config: &BinaryObservabilityConfig,
) -> Result<TraceGuard> {
    if let Some(endpoint) = &config.otlp_endpoint {
        // Create OpenTelemetry resource with service metadata
        let resource = std::borrow::Cow::Owned(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                binary_config.service_name,
            ),
        ]));

        // Create instrumentation library metadata
        let instrumentation_lib = opentelemetry::InstrumentationLibrary::builder("stepflow")
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_schema_url(opentelemetry_semantic_conventions::SCHEMA_URL)
            .build();

        // Create OTLP exporter
        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint)
            .with_timeout(std::time::Duration::from_secs(5))
            .build_span_exporter()
            .map_err(|e| {
                error_stack::report!(ObservabilityError::OtlpInitError)
                    .attach_printable(format!("Failed to create OTLP exporter: {}", e))
            })?;

        // Create OpenTelemetry reporter
        let reporter = fastrace_opentelemetry::OpenTelemetryReporter::new(
            exporter,
            opentelemetry::trace::SpanKind::Internal,
            resource,
            instrumentation_lib,
        );

        fastrace::set_reporter(reporter, fastrace::collector::Config::default());
    } else {
        // Console reporter for development (no OTLP endpoint configured)
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
