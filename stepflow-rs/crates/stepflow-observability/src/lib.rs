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

use opentelemetry_otlp::{WithExportConfig, WithTonicConfig};

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

    /// Log output destination
    #[arg(long, default_value = "stdout", env = "STEPFLOW_LOG_DESTINATION")]
    pub log_destination: LogDestinationType,

    /// Log output format (only applies to stdout and file destinations)
    #[arg(long, default_value = "text", env = "STEPFLOW_LOG_FORMAT")]
    pub log_format: LogFormat,

    /// Log file path (only applies when log_destination is file)
    #[arg(long, env = "STEPFLOW_LOG_FILE")]
    pub log_file: Option<std::path::PathBuf>,

    /// Enable distributed tracing
    #[arg(long, env = "STEPFLOW_TRACE_ENABLED")]
    pub trace_enabled: bool,

    /// OTLP endpoint (e.g., "http://localhost:4317")
    /// Required when log_destination is otlp or when trace_enabled is true
    #[arg(long, env = "STEPFLOW_OTLP_ENDPOINT")]
    pub otlp_endpoint: Option<String>,
}

impl ObservabilityConfig {
    /// Validate the configuration for internal consistency
    pub fn validate(&self) -> Result<()> {
        // Validate log_destination consistency
        match self.log_destination {
            LogDestinationType::File => {
                if self.log_file.is_none() {
                    return Err(
                        error_stack::report!(ObservabilityError::ConfigValidationError)
                            .attach_printable("log_destination is 'file' but log_file is not set"),
                    );
                }
            }
            LogDestinationType::Stdout => {
                if self.log_file.is_some() {
                    return Err(
                        error_stack::report!(ObservabilityError::ConfigValidationError)
                            .attach_printable("log_destination is 'stdout' but log_file is set"),
                    );
                }
            }
            LogDestinationType::Otlp => {
                if self.log_file.is_some() {
                    return Err(
                        error_stack::report!(ObservabilityError::ConfigValidationError)
                            .attach_printable("log_destination is 'otlp' but log_file is set"),
                    );
                }
                if self.otlp_endpoint.is_none() {
                    return Err(
                        error_stack::report!(ObservabilityError::ConfigValidationError)
                            .attach_printable(
                                "log_destination is 'otlp' but otlp_endpoint is not set",
                            ),
                    );
                }
            }
        }

        // Validate trace_enabled requires otlp_endpoint
        if self.trace_enabled && self.otlp_endpoint.is_none() {
            return Err(
                error_stack::report!(ObservabilityError::ConfigValidationError)
                    .attach_printable("trace_enabled is true but otlp_endpoint is not set"),
            );
        }

        Ok(())
    }

    /// Get the effective log destination for internal use
    fn log_destination(&self) -> LogDestination<'_> {
        match self.log_destination {
            LogDestinationType::Stdout => LogDestination::Stdout,
            LogDestinationType::File => {
                LogDestination::File(self.log_file.as_ref().expect("Validated in validate()"))
            }
            LogDestinationType::Otlp => LogDestination::OpenTelemetry,
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

/// Log destination type (CLI argument value)
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum LogDestinationType {
    /// Log to stdout
    Stdout,
    /// Log to a file
    File,
    /// Log to OpenTelemetry (OTLP)
    Otlp,
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
///     ObservabilityConfig, BinaryObservabilityConfig, LogFormat, LogDestinationType, init_observability
/// };
///
/// let config = ObservabilityConfig {
///     log_level: log::LevelFilter::Info,
///     other_log_level: Some(log::LevelFilter::Warn),  // Separate level for dependencies
///     log_destination: LogDestinationType::Stdout,
///     log_format: LogFormat::Json,
///     log_file: None,
///     trace_enabled: true,
///     otlp_endpoint: Some("http://localhost:4317".to_string()),
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
    // Validate configuration first
    config.validate()?;

    // Initialize tracing first (so logger can access trace context)
    let trace_guard = if config.trace_enabled {
        Some(init_tracing(config, &binary_config)?)
    } else {
        None
    };

    // Initialize logging with trace context integration
    let log_guard = init_logging(config, &binary_config)?;

    Ok(ObservabilityGuard {
        log_guard: Some(log_guard),
        trace_guard,
        closed: false,
    })
}

fn init_logging(
    config: &ObservabilityConfig,
    binary_config: &BinaryObservabilityConfig,
) -> Result<LogGuard> {
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

            // Dispatch 1: stepflow_* crates at the main log level
            builder = add_dispatch(
                &format!("stepflow_={}", level_to_str(config.log_level)),
                config,
                binary_config,
                builder,
            );

            // Dispatch 2: non-stepflow crates at the other log level
            builder = add_dispatch(
                &format!("{},stepflow_=off", level_to_str(other_level)),
                config,
                binary_config,
                builder,
            );
        }
        None => {
            builder = add_dispatch(
                level_to_str(config.log_level),
                config,
                binary_config,
                builder,
            );
        }
    }

    builder.apply();

    Ok(LogGuard)
}

#[must_use]
fn add_dispatch(
    filter_spec: &str,
    config: &ObservabilityConfig,
    binary_config: &BinaryObservabilityConfig,
    builder: logforth::starter_log::LogStarterBuilder,
) -> logforth::starter_log::LogStarterBuilder {
    use logforth::diagnostic::FastraceDiagnostic;
    use logforth::filter::env_filter::EnvFilterBuilder;

    let filter = EnvFilterBuilder::from_spec(filter_spec).build();
    let destination = config.log_destination();
    let format = config.log_format;

    builder.dispatch(|d| {
        let mut d = d.filter(filter).diagnostic(FastraceDiagnostic::default());

        if binary_config.include_run_diagnostic {
            d = d.diagnostic(RunDiagnostic);
        }

        d.append(create_appender(
            destination,
            format,
            config.otlp_endpoint.as_deref(),
            binary_config.service_name,
        ))
    })
}

/// Create an appender based on configuration
fn create_appender(
    destination: LogDestination,
    format: LogFormat,
    otlp_endpoint: Option<&str>,
    service_name: &'static str,
) -> Box<dyn logforth::Append> {
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
            use logforth_append_opentelemetry::OpentelemetryLogBuilder;
            use opentelemetry_otlp::LogExporter;
            use opentelemetry_otlp::WithExportConfig;

            // Use Zstd compression for efficient log transmission
            // Provides ~50-80% bandwidth reduction with minimal CPU overhead
            let log_exporter = LogExporter::builder()
                .with_tonic()
                .with_endpoint(otlp_endpoint.expect("Endpoint required for OTLP logging"))
                .with_timeout(std::time::Duration::from_secs(5))
                .with_compression(opentelemetry_otlp::Compression::Zstd)
                .build()
                .expect("Failed to create OTLP log exporter");

            let builder = OpentelemetryLogBuilder::new(service_name, log_exporter)
                // Add service.name as a resource attribute per OpenTelemetry spec
                .label(
                    opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                    service_name,
                )
                .label(
                    opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
                    env!("CARGO_PKG_VERSION"),
                );

            // TODO: Add env, etc. labels (`builder.label("env", "production");`)
            Box::new(builder.build())
        }
    }
}

fn init_tracing(
    config: &ObservabilityConfig,
    binary_config: &BinaryObservabilityConfig,
) -> Result<TraceGuard> {
    if let Some(endpoint) = &config.otlp_endpoint {
        // Create OpenTelemetry resource with service metadata
        let resource = std::borrow::Cow::Owned(
            opentelemetry_sdk::Resource::builder()
                .with_service_name(binary_config.service_name)
                .build(),
        );

        // Create instrumentation library metadata
        let instrumentation_lib = opentelemetry::InstrumentationScope::builder("stepflow")
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_schema_url(opentelemetry_semantic_conventions::SCHEMA_URL)
            .build();

        // Create OTLP trace exporter with Zstd compression
        // Provides ~50-80% bandwidth reduction with minimal CPU overhead
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_timeout(std::time::Duration::from_secs(5))
            .with_compression(opentelemetry_otlp::Compression::Zstd)
            .build()
            .map_err(|e| {
                error_stack::report!(ObservabilityError::OtlpInitError)
                    .attach_printable(format!("Failed to create OTLP exporter: {}", e))
            })?;

        // Create OpenTelemetry reporter
        let reporter = fastrace_opentelemetry::OpenTelemetryReporter::new(
            exporter,
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
///
/// # Important
///
/// You must call [`ObservabilityGuard::close()`] before dropping this guard.
/// Failing to do so will result in a panic, as the guard needs to flush telemetry
/// data while the async runtime is still available.
///
/// # Example
///
/// ```no_run
/// # use stepflow_observability::{ObservabilityConfig, BinaryObservabilityConfig, LogDestinationType, init_observability};
/// # let config = ObservabilityConfig {
/// #     log_level: log::LevelFilter::Info,
/// #     other_log_level: None,
/// #     log_destination: LogDestinationType::Stdout,
/// #     log_format: stepflow_observability::LogFormat::Text,
/// #     log_file: None,
/// #     trace_enabled: false,
/// #     otlp_endpoint: None,
/// # };
/// # let binary_config = BinaryObservabilityConfig {
/// #     service_name: "my-service",
/// #     include_run_diagnostic: true,
/// # };
/// let guard = init_observability(&config, binary_config).unwrap();
///
/// // ... application logic ...
///
/// // Explicitly close before dropping
/// guard.close().expect("Failed to flush observability data");
/// ```
pub struct ObservabilityGuard {
    #[allow(dead_code)] // Kept for future log flushing functionality
    log_guard: Option<LogGuard>,
    trace_guard: Option<TraceGuard>,
    closed: bool,
}

impl ObservabilityGuard {
    /// Explicitly close the observability guard and flush all pending telemetry.
    ///
    /// This must be called before the guard is dropped. It will flush both logs
    /// and traces to their respective destinations.
    ///
    /// # Errors
    ///
    /// Returns an error if flushing fails. This can happen if:
    /// - The OTLP endpoint is unreachable
    /// - The tokio runtime is shutting down
    /// - Log/trace exporters encounter errors
    pub async fn close(mut self) -> Result<()> {
        self.closed = true;

        // Flush logs first - this calls the logger's flush() method which will
        // flush all appenders including OTLP
        log::logger().flush();

        // Flush traces
        if self.trace_guard.is_some() {
            fastrace::flush();

            // Yield to the tokio runtime to allow async OTLP export to complete
            // The flush() queues spans but the actual network I/O is async
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        Ok(())
    }

    /// Leak the guard without flushing.
    ///
    /// This is intended for tests where we don't care about flushing telemetry
    /// at the end and want to avoid the panic on drop.
    #[doc(hidden)]
    pub fn leak(mut self) {
        self.closed = true;
        std::mem::forget(self);
    }
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        if !self.closed {
            panic!(
                "ObservabilityGuard must be explicitly closed by calling .close() before dropping. \
                This ensures telemetry data is flushed while the async runtime is still available."
            );
        }
    }
}

struct LogGuard;
struct TraceGuard;

/// Errors that can occur during observability initialization
#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error("Failed to initialize logging")]
    LogInitError,
    #[error("Failed to initialize tracing")]
    TraceInitError,
    #[error("Failed to initialize OTLP")]
    OtlpInitError,
    #[error("Invalid observability configuration")]
    ConfigValidationError,
}

pub type Result<T> = std::result::Result<T, error_stack::Report<ObservabilityError>>;
