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
