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

This module sets up OpenTelemetry tracing and logging with OTLP export.
Configuration is done via environment variables:
- STEPFLOW_OTLP_ENDPOINT: OTLP endpoint URL (e.g., http://localhost:4317)
- STEPFLOW_SERVICE_NAME: Service name for traces/logs (default: stepflow-python)
- STEPFLOW_TRACE_ENABLED: Enable tracing (default: true if OTLP endpoint set)
"""

import logging
import os
import sys
import typing

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
    from stepflow_py.generated_protocol import ObservabilityContext

class ObservabilityConfig:
    """Configuration for OpenTelemetry observability.

    Reads configuration from environment variables:
    - STEPFLOW_OTLP_ENDPOINT: OTLP endpoint URL (required for tracing/logging)
    - STEPFLOW_SERVICE_NAME: Service name (default: stepflow-python)
    - STEPFLOW_TRACE_ENABLED: Enable tracing (default: true if endpoint set)
    """

    def __init__(self) -> None:
        self.otlp_endpoint = os.environ.get("STEPFLOW_OTLP_ENDPOINT")
        self.service_name = os.environ.get("STEPFLOW_SERVICE_NAME", "stepflow-python")
        self.trace_enabled = (
            os.environ.get("STEPFLOW_TRACE_ENABLED", "true").lower() == "true"
        )

        # Determine if observability is enabled
        self.enabled = bool(self.otlp_endpoint) and self.trace_enabled

    def __repr__(self) -> str:
        return (
            f"ObservabilityConfig(enabled={self.enabled}, "
            f"service_name={self.service_name}, "
            f"otlp_endpoint={self.otlp_endpoint})"
        )


def setup_observability(config: ObservabilityConfig | None = None) -> None:
    """Initialize OpenTelemetry tracing and logging.

    Args:
        config: Observability configuration. If None, reads from environment.

    Raises:
        RuntimeError: If OTLP endpoint is configured but initialization fails.
    """
    if config is None:
        config = ObservabilityConfig()

    if not config.enabled:
        logger.info("Observability disabled (no OTLP endpoint configured)")
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

        # Setup logging if available
        if OTLP_LOGGING_AVAILABLE:
            logger_provider = LoggerProvider(resource=resource)
            log_exporter = OTLPLogExporter(endpoint=config.otlp_endpoint, insecure=True)
            logger_provider.add_log_record_processor(
                BatchLogRecordProcessor(log_exporter)
            )
            set_logger_provider(logger_provider)

            # Add OTLP logging handler to root logger
            handler = LoggingHandler(
                level=logging.INFO, logger_provider=logger_provider
            )
            logging.getLogger().addHandler(handler)

            logger.info("OpenTelemetry logging initialized")
        else:
            logger.warning(
                "OpenTelemetry logging not available "
                "(requires opentelemetry-sdk>=1.20.0)"
            )

    except Exception as e:
        # Per user requirements: if OTLP endpoint configured but fails → FAIL
        logger.error(f"Failed to initialize observability: {e}")
        sys.exit(1)


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


def get_current_observability_context(
    run_id: str | None = None,
    flow_id: str | None = None,
    step_id: str | None = None,
) -> "ObservabilityContext | None":  # type: ignore
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
    from stepflow_py.generated_protocol import ObservabilityContext

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
