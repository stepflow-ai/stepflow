#!/usr/bin/env python3
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

"""Standalone Stepflow component server for Docling integration.

This script can be run directly and provides a simple entry point.
"""

from stepflow_docling.server.docling_server import StepflowDoclingServer


def main():
    """Main entry point for the Docling component server.

    Configure via environment variables:
    - DOCLING_SERVE_URL: URL of the docling-serve instance (default: http://localhost:5001)
    - DOCLING_SERVE_API_KEY: Optional API key for docling-serve authentication
    - STEPFLOW_LOG_LEVEL: Log level (DEBUG, INFO, WARNING, ERROR, default: INFO)
    - STEPFLOW_LOG_DESTINATION: Log destination (stderr, file, otlp)
    - STEPFLOW_OTLP_ENDPOINT: OTLP endpoint for tracing/logging
    - STEPFLOW_SERVICE_NAME: Service name (default: stepflow-docling)
    """
    # Initialize observability (tracing, logging) before starting server
    from stepflow_py.worker.observability import setup_observability

    setup_observability()

    server = StepflowDoclingServer()
    server.run()


if __name__ == "__main__":
    """Run the Docling component server."""
    main()
