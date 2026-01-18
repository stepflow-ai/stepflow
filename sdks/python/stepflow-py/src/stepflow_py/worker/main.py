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

import argparse
import asyncio

from stepflow_py.worker.server import StepflowServer

# Create server instance
server = StepflowServer()


def main():
    # Initialize observability before anything else
    # Configuration via environment variables:
    #   STEPFLOW_SERVICE_NAME: Service name for traces/logs (default: stepflow-python)
    #   STEPFLOW_LOG_LEVEL: Log level (DEBUG, INFO, WARNING, ERROR, default: INFO)
    #   STEPFLOW_LOG_DESTINATION: Where to log (stderr, file, otlp)
    #                             Default: "otlp" if OTLP endpoint set, else "stderr"
    #   STEPFLOW_LOG_FILE: File path for file logging
    #   STEPFLOW_OTLP_ENDPOINT: OTLP endpoint for trace/log export
    #   STEPFLOW_TRACE_ENABLED: Enable tracing (default: true)
    from stepflow_py.worker.observability import setup_observability

    setup_observability()

    parser = argparse.ArgumentParser(
        description="Stepflow Python SDK Server",
        epilog="""
Environment variables:
  STEPFLOW_SERVICE_NAME      Service name for observability (default: stepflow-python)
  STEPFLOW_LOG_LEVEL         Log level: DEBUG, INFO, WARNING, ERROR (default: INFO)
  STEPFLOW_LOG_DESTINATION   Log destination: stderr, file, otlp, or comma-separated
                             Default: "otlp" if OTLP endpoint set, else "stderr"
  STEPFLOW_LOG_FILE          File path if file logging is enabled
  STEPFLOW_OTLP_ENDPOINT     OTLP endpoint for tracing/logging
  STEPFLOW_TRACE_ENABLED     Enable tracing: true, false (default: true)
        """,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--port",
        type=int,
        default=0,
        help="HTTP port (0 for auto-assign, default: 0)",
    )
    parser.add_argument(
        "--host",
        type=str,
        default="127.0.0.1",
        help="HTTP host (default: 127.0.0.1)",
    )

    args = parser.parse_args()

    # Start HTTP server with specified host/port
    asyncio.run(server.run(host=args.host, port=args.port))


if __name__ == "__main__":
    main()
