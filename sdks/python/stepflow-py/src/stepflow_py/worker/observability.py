# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License. You may obtain a copy of
# the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations under
# the License.

"""Observability configuration for distributed tracing and logging.

This module sets up OpenTelemetry tracing and Python logging with structured output.
Configuration is done via environment variables:
- STEPFLOW_OTLP_ENDPOINT: OTLP endpoint URL (e.g., http://localhost:4317)
- STEPFLOW_SERVICE_NAME: Service name for traces/logs (default: stepflow-python)
- STEPFLOW_TRACE_ENABLED: Enable tracing (default: true if OTLP endpoint set)
- STEPFLOW_LOG_LEVEL: Log level (DEBUG, INFO, WARNING, ERROR, default: INFO)
- STEPFLOW_LOG_DESTINATION: Where to log (stderr, file, otlp, or comma-separated)
                             Default: "otlp" if OTLP endpoint set, else "stderr"
- STEPFLOW_LOG_FILE: File path if file logging is enabled
"""

import contextvars
import json
import logging
import os
import sys
import typing
from datetime import UTC, datetime

from opentelemetry import trace
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor

# OpenTelemetry logging support
try:
    from opentelemetry._logs import set_logger_provider
    from opentelemetry.exporter.otlp.proto.grpc._log_exporter import OTLPLogExporter
    from opentelemetry.sdk._logs import LoggerProvider, LoggingHandler
    from opentelemetry.sdk._logs.export import BatchLogRecordProcessor

    OTLP_LOGGING_AVAILABLE = True
except ImportError:
    OTLP_LOGGING_AVAILABLE = False

logger = logging.getLogger(__name__)

if typing.TYPE_CHECKING:
    from stepflow_py.worker.generated_protocol import ObservabilityContext

# Context variables for diagnostic context
_diagnostic_context: contextvars.ContextVar[dict[str, str] | None] = (
    contextvars.ContextVar("diagnostic_context", default=None)
)


class ObservabilityConfig:
    """Configuration for OpenTelemetry observability and logging.

    Reads configuration from environment variables:
    - STEPFLOW_OTLP_ENDPOINT: OTLP endpoint URL (required for tracing/OTLP logging)
    - STEPFLOW_SERVICE_NAME: Service name (default: stepflow-python)
    - STEPFLOW_TRACE_ENABLED: Enable tracing (default: true if endpoint set)
    - STEPFLOW_LOG_LEVEL: Log level (DEBUG, INFO, WARNING, ERROR, default: INFO)
    - STEPFLOW_LOG_DESTINATION: Where to log (stderr, file, otlp, comma-separated)
                                 Default: "otlp" if OTLP endpoint set, else "stderr"
    - STEPFLOW_LOG_FILE: File path if file logging is enabled
    """

    def __init__(self) -> None:
        self.otlp_endpoint = os.environ.get("STEPFLOW_OTLP_ENDPOINT")
        self.service_name = os.environ.get("STEPFLOW_SERVICE_NAME", "stepflow-python")
        self.trace_enabled = (
            os.environ.get("STEPFLOW_TRACE_ENABLED", "true").lower() == "true"
        )

        # Logging configuration
        log_level_str = os.environ.get("STEPFLOW_LOG_LEVEL", "INFO").upper()
        self.log_level = getattr(logging, log_level_str, logging.INFO)

        # Parse log destinations with intelligent defaults:
        # - If STEPFLOW_LOG_DESTINATION is set, use that value
        # - If OTLP endpoint is configured, default to "otlp"
        # - Otherwise, default to "stderr"
        dest_str = os.environ.get("STEPFLOW_LOG_DESTINATION")
        if dest_str is None:
            # Intelligent default: OTLP if endpoint configured, otherwise stderr
            dest_str = "otlp" if self.otlp_endpoint else "stderr"
        self.log_destinations = [d.strip() for d in dest_str.lower().split(",")]

        # File logging configuration
        self.log_file = os.environ.get("STEPFLOW_LOG_FILE")

        # Determine if observability is enabled
        self.enabled = bool(self.otlp_endpoint) and self.trace_enabled

    def __repr__(self) -> str:
        return (
            f"ObservabilityConfig(enabled={self.enabled}, "
            f"service_name={self.service_name}, "
            f"otlp_endpoint={self.otlp_endpoint}, "
            f"log_level={logging.getLevelName(self.log_level)}, "
            f"log_destinations={self.log_destinations})"
        )


class DiagnosticContextFilter(logging.Filter):
    """Filter that injects diagnostic context into log records."""

    def filter(self, record: logging.LogRecord) -> bool:
        """Add diagnostic context fields to the log record."""
        context = _diagnostic_context.get() or {}

        # Add context fields to record
        record.flow_id = context.get("flow_id", "")
        record.run_id = context.get("run_id", "")
        record.step_id = context.get("step_id", "")
        record.trace_id = context.get("trace_id", "")
        record.span_id = context.get("span_id", "")

        return True


class StructuredJsonFormatter(logging.Formatter):
    """JSON formatter with diagnostic context fields."""

    def format(self, record: logging.LogRecord) -> str:
        """Format log record as JSON with diagnostic context."""
        log_entry = {
            "timestamp": datetime.fromtimestamp(record.created, tz=UTC).isoformat(),
            "level": record.levelname,
            "message": record.getMessage(),
            "logger": record.name,
            "module": record.module,
            "function": record.funcName,
            "line": record.lineno,
        }

        # Add diagnostic context if present
        if hasattr(record, "flow_id") and record.flow_id:
            log_entry["flow_id"] = record.flow_id
        if hasattr(record, "run_id") and record.run_id:
            log_entry["run_id"] = record.run_id
        if hasattr(record, "step_id") and record.step_id:
            log_entry["step_id"] = record.step_id
        if hasattr(record, "trace_id") and record.trace_id:
            log_entry["trace_id"] = record.trace_id
        if hasattr(record, "span_id") and record.span_id:
            log_entry["span_id"] = record.span_id

        # Add exception info if present
        if record.exc_info:
            log_entry["exception"] = self.formatException(record.exc_info)

        return json.dumps(log_entry)


def setup_observability(config: ObservabilityConfig | None = None) -> None:
    """Initialize OpenTelemetry tracing and Python logging.

    Args:
        config: Observability configuration. If None, reads from environment.

    Raises:
        SystemExit: If OTLP endpoint is configured but initialization fails.
    """
    if config is None:
        config = ObservabilityConfig()

    # Configure Python logging (always enabled)
    _setup_logging(config)

    # Setup OpenTelemetry tracing if enabled
    if not config.enabled:
        logger.info("OpenTelemetry tracing disabled (no OTLP endpoint configured)")
        return

    try:
        # Create resource with service name
        resource = Resource.create({"service.name": config.service_name})

        # Setup tracing
        tracer_provider = TracerProvider(resource=resource)
        trace_exporter = OTLPSpanExporter(endpoint=config.otlp_endpoint, insecure=True)
        tracer_provider.add_span_processor(BatchSpanProcessor(trace_exporter))
        trace.set_tracer_provider(tracer_provider)

        logger.info(
            f"OpenTelemetry tracing initialized: service={config.service_name}, "
            f"endpoint={config.otlp_endpoint}"
        )

        # Setup OTLP logging if "otlp" destination is specified
        if "otlp" in config.log_destinations and OTLP_LOGGING_AVAILABLE:
            logger_provider = LoggerProvider(resource=resource)
            log_exporter = OTLPLogExporter(endpoint=config.otlp_endpoint, insecure=True)
            logger_provider.add_log_record_processor(
                BatchLogRecordProcessor(log_exporter)
            )
            set_logger_provider(logger_provider)

            # Add OTLP logging handler to root logger
            handler = LoggingHandler(
                level=config.log_level, logger_provider=logger_provider
            )
            logging.getLogger().addHandler(handler)

            logger.info("OpenTelemetry logging initialized")
        elif "otlp" in config.log_destinations and not OTLP_LOGGING_AVAILABLE:
            logger.warning(
                "OpenTelemetry logging requested but not available "
                "(requires opentelemetry-sdk>=1.20.0)"
            )

    except Exception as e:
        # Per user requirements: if OTLP endpoint configured but fails → FAIL
        logger.error(f"Failed to initialize observability: {e}")
        sys.exit(1)


def _setup_logging(config: ObservabilityConfig) -> None:
    """Configure Python logging with structured output.

    Args:
        config: Observability configuration with log settings.
    """
    # Create root logger
    root_logger = logging.getLogger()
    root_logger.setLevel(config.log_level)

    # Remove existing handlers to avoid duplicates
    root_logger.handlers.clear()

    # Create diagnostic context filter
    context_filter = DiagnosticContextFilter()

    # Configure handlers based on destinations
    for destination in config.log_destinations:
        if destination == "stderr":
            stderr_handler = logging.StreamHandler(sys.stderr)
            stderr_handler.setLevel(config.log_level)
            stderr_handler.setFormatter(StructuredJsonFormatter())
            stderr_handler.addFilter(context_filter)
            root_logger.addHandler(stderr_handler)

        elif destination == "file":
            if not config.log_file:
                logger.warning("File logging requested but STEPFLOW_LOG_FILE not set")
                continue
            file_handler = logging.FileHandler(config.log_file)
            file_handler.setLevel(config.log_level)
            file_handler.setFormatter(StructuredJsonFormatter())
            file_handler.addFilter(context_filter)
            root_logger.addHandler(file_handler)

        elif destination == "otlp":
            # OTLP logging is handled separately in setup_observability
            pass

        else:
            logger.warning(f"Unknown log destination: {destination}")


def get_tracer(name: str) -> trace.Tracer:
    """Get a tracer for the given name.

    Args:
        name: The tracer name (typically module name).

    Returns:
        OpenTelemetry Tracer instance.
    """
    return trace.get_tracer(name)


def extract_trace_context(
    trace_id: str | None, span_id: str | None
) -> trace.SpanContext | None:
    """Extract OpenTelemetry SpanContext from trace_id and span_id strings.

    Args:
        trace_id: 128-bit trace ID as hex string (32 chars).
        span_id: 64-bit span ID as hex string (16 chars).

    Returns:
        SpanContext if both IDs provided, None otherwise.
    """
    if not trace_id or not span_id:
        return None

    try:
        # Convert hex strings to integers
        trace_id_int = int(trace_id, 16)
        span_id_int = int(span_id, 16)

        # Create SpanContext with remote flag set
        return trace.SpanContext(
            trace_id=trace_id_int,
            span_id=span_id_int,
            is_remote=True,
            trace_flags=trace.TraceFlags(0x01),  # SAMPLED flag
        )
    except (ValueError, TypeError) as e:
        logger.warning(f"Failed to parse trace context: {e}")
        return None


def set_diagnostic_context(
    flow_id: str | None = None,
    run_id: str | None = None,
    step_id: str | None = None,
) -> None:
    """Set diagnostic context for structured logging.

    This sets context variables that will be automatically included in all
    log messages within the current async context.

    Args:
        flow_id: Flow ID to include in logs.
        run_id: Run ID to include in logs.
        step_id: Step ID to include in logs.
    """
    context = {}

    if flow_id:
        context["flow_id"] = flow_id
    if run_id:
        context["run_id"] = run_id
    if step_id:
        context["step_id"] = step_id

    # Extract trace context from current span if available
    current_span = trace.get_current_span()
    span_context = current_span.get_span_context()
    if span_context.is_valid:
        context["trace_id"] = format(span_context.trace_id, "032x")
        context["span_id"] = format(span_context.span_id, "016x")

    _diagnostic_context.set(context)


def get_diagnostic_context() -> dict[str, str]:
    """Get the current diagnostic context.

    Returns:
        Dictionary with current diagnostic context fields.
    """
    return _diagnostic_context.get() or {}


def get_current_observability_context(
    run_id: str | None = None,
    flow_id: str | None = None,
    step_id: str | None = None,
) -> "ObservabilityContext | None":
    """Capture the current OpenTelemetry span context for bidirectional requests.

    This extracts the trace_id and span_id from the current active span,
    allowing bidirectional requests to properly propagate trace context.

    Args:
        run_id: Optional run ID to include in the context.
        flow_id: Optional flow ID to include in the context.
        step_id: Optional step ID to include in the context.

    Returns:
        ObservabilityContext with current span's trace_id and span_id,
        or None if no span is active.
    """
    from stepflow_py.worker.generated_protocol import ObservabilityContext

    # Get the current span
    current_span = trace.get_current_span()

    # Check if span is valid and sampled
    span_context = current_span.get_span_context()
    if not span_context.is_valid:
        return None

    # Extract trace_id and span_id as hex strings
    trace_id_hex = format(span_context.trace_id, "032x")
    span_id_hex = format(span_context.span_id, "016x")

    return ObservabilityContext(
        trace_id=trace_id_hex,
        span_id=span_id_hex,
        run_id=run_id,
        flow_id=flow_id,
        step_id=step_id,
    )
