#!/usr/bin/env python3
"""Standalone Stepflow component server for Docling step worker.

This script can be run directly and provides a simple entry point.
"""

from docling_step_worker.server import DoclingStepWorkerServer


def main() -> None:
    """Main entry point for the Docling step worker server.

    Configure via environment variables:
    - STEPFLOW_LOG_LEVEL: Log level (DEBUG, INFO, WARNING, ERROR, default: INFO)
    - STEPFLOW_LOG_DESTINATION: Log destination (stderr, file, otlp)
    - STEPFLOW_OTLP_ENDPOINT: OTLP endpoint for tracing/logging
    - STEPFLOW_SERVICE_NAME: Service name (default: docling-step-worker)
    """
    from stepflow_py.worker.observability import setup_observability

    setup_observability()

    server = DoclingStepWorkerServer()
    server.run()


if __name__ == "__main__":
    main()
