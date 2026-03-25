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
import os

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
  STEPFLOW_TRANSPORT         Transport mode: grpc, nats (default: grpc)
  STEPFLOW_TASKS_URL         TasksService gRPC address (default: localhost:7837)
  STEPFLOW_NATS_URL          NATS server URL (default: nats://localhost:4222)
  STEPFLOW_NATS_STREAM       NATS JetStream stream name (default: STEPFLOW_TASKS)
  STEPFLOW_NATS_CONSUMER     NATS durable consumer name (default: stepflow-default)
  STEPFLOW_QUEUE_NAME        Queue name for gRPC transport (default: python)
  STEPFLOW_MAX_CONCURRENT    Max concurrent task executions (default: 4)
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
    transport_group = parser.add_mutually_exclusive_group()
    transport_group.add_argument(
        "--grpc",
        action="store_true",
        help="Use gRPC pull-based transport (default)",
    )
    transport_group.add_argument(
        "--nats",
        action="store_true",
        help="Use NATS JetStream transport",
    )
    transport_group.add_argument(
        "--vsock-port",
        type=int,
        help="Use vsock transport on specified port (for Firecracker VMs)",
    )
    transport_group.add_argument(
        "--socket",
        type=str,
        help="Use Unix socket transport at specified path (dev mode)",
    )
    parser.add_argument(
        "--tasks-url",
        type=str,
        default=os.environ.get("STEPFLOW_TASKS_URL", "localhost:7837"),
        help="TasksService gRPC address (env: STEPFLOW_TASKS_URL)",
    )
    parser.add_argument(
        "--nats-url",
        type=str,
        default=os.environ.get("STEPFLOW_NATS_URL", "nats://localhost:4222"),
        help="NATS server URL (env: STEPFLOW_NATS_URL)",
    )
    parser.add_argument(
        "--nats-stream",
        type=str,
        default=os.environ.get("STEPFLOW_NATS_STREAM", "STEPFLOW_TASKS"),
        help="NATS JetStream stream name (env: STEPFLOW_NATS_STREAM)",
    )
    parser.add_argument(
        "--nats-consumer",
        type=str,
        default=os.environ.get("STEPFLOW_NATS_CONSUMER", "stepflow-default"),
        help="NATS durable consumer name (env: STEPFLOW_NATS_CONSUMER)",
    )
    parser.add_argument(
        "--queue-name",
        type=str,
        default=os.environ.get("STEPFLOW_QUEUE_NAME", "python"),
        help="Queue name for gRPC transport (env: STEPFLOW_QUEUE_NAME)",
    )
    parser.add_argument(
        "--max-concurrent",
        type=int,
        default=int(os.environ.get("STEPFLOW_MAX_CONCURRENT", "4")),
        help="Max concurrent task executions (env: STEPFLOW_MAX_CONCURRENT)",
    )

    args = parser.parse_args()

    # Transport precedence: CLI flags > STEPFLOW_TRANSPORT env > default (grpc)
    transport_env = os.environ.get("STEPFLOW_TRANSPORT", "grpc").lower()
    if args.vsock_port is not None or args.socket is not None:
        transport = "vsock"
    elif args.nats:
        transport = "nats"
    elif args.grpc:
        transport = "grpc"
    else:
        transport = transport_env

    if transport == "vsock":
        # Start vsock/socket worker
        from stepflow_py.worker.vsock_worker import run_vsock_worker

        asyncio.run(
            run_vsock_worker(
                server=server,
                vsock_port=args.vsock_port,
                socket_path=args.socket,
                max_concurrent=args.max_concurrent,
            )
        )
    elif transport == "nats":
        # Start NATS JetStream worker
        from stepflow_py.worker.nats_worker import run_nats_worker

        asyncio.run(
            run_nats_worker(
                server=server,
                nats_url=args.nats_url,
                stream=args.nats_stream,
                consumer=args.nats_consumer,
                max_concurrent=args.max_concurrent,
            )
        )
    else:
        # Start gRPC pull-based worker (default)
        from stepflow_py.worker.grpc_worker import run_grpc_worker

        asyncio.run(
            run_grpc_worker(
                server=server,
                tasks_url=args.tasks_url,
                queue_name=args.queue_name,
                max_concurrent=args.max_concurrent,
            )
        )


if __name__ == "__main__":
    main()
