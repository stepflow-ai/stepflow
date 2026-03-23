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

"""Standalone Stepflow gRPC worker for Docling step worker.

This script can be run directly and provides a simple entry point.
"""

import asyncio
import os

from docling_step_worker.server import DoclingStepWorkerServer


def main() -> None:
    """Main entry point for the Docling step worker server.

    Configure via environment variables:
    - STEPFLOW_TASKS_URL: TasksService gRPC address (default: localhost:7837)
    - STEPFLOW_BLOB_URL: Blob service gRPC address
    - STEPFLOW_QUEUE_NAME: Queue name for gRPC transport (default: python)
    - STEPFLOW_MAX_CONCURRENT: Max concurrent task executions (default: 4)
    - STEPFLOW_LOG_LEVEL: Log level (DEBUG, INFO, WARNING, ERROR, default: INFO)
    - STEPFLOW_LOG_DESTINATION: Log destination (stderr, file, otlp)
    - STEPFLOW_OTLP_ENDPOINT: OTLP endpoint for tracing/logging
    - STEPFLOW_SERVICE_NAME: Service name (default: docling-step-worker)
    """
    from stepflow_py.worker.grpc_worker import run_grpc_worker
    from stepflow_py.worker.observability import setup_observability

    setup_observability()

    server = DoclingStepWorkerServer()
    asyncio.run(
        run_grpc_worker(
            server=server.server,
            tasks_url=os.environ.get("STEPFLOW_TASKS_URL", "localhost:7837"),
            queue_name=os.environ.get("STEPFLOW_QUEUE_NAME", "python"),
            max_concurrent=int(os.environ.get("STEPFLOW_MAX_CONCURRENT", "4")),
        )
    )


if __name__ == "__main__":
    main()
