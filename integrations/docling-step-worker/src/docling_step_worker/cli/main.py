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

"""Main CLI entry point for Docling step worker."""

from __future__ import annotations

import asyncio
import os
import sys

import click
from dotenv import load_dotenv

from docling_step_worker.server import DoclingStepWorkerServer


@click.group()
@click.version_option()
def main() -> None:
    """Docling Step Worker CLI."""
    load_dotenv()


@main.command()
@click.option(
    "--tasks-url",
    default=None,
    help="TasksService gRPC address (env: STEPFLOW_TASKS_URL, default: localhost:7837)",
)
@click.option(
    "--queue-name",
    default=None,
    help="Queue name for gRPC transport (env: STEPFLOW_QUEUE_NAME, default: python)",
)
@click.option(
    "--max-concurrent",
    default=None,
    type=int,
    help="Max concurrent task executions (env: STEPFLOW_MAX_CONCURRENT, default: 4)",
)
def serve(
    tasks_url: str | None,
    queue_name: str | None,
    max_concurrent: int | None,
) -> None:
    """Start the Docling step worker as a gRPC pull-based worker.

    Environment variables:
        STEPFLOW_TASKS_URL: TasksService gRPC address (default: localhost:7837)
        STEPFLOW_BLOB_URL: Blob service gRPC address
        STEPFLOW_LOG_LEVEL: Log level (default: INFO)
        STEPFLOW_LOG_DESTINATION: Log destination (stderr, file, otlp)
    """
    from stepflow_py.worker.grpc_worker import run_grpc_worker
    from stepflow_py.worker.observability import setup_observability

    setup_observability()

    try:
        click.echo("Starting Docling step worker (gRPC)...", err=True)

        server = DoclingStepWorkerServer()
        asyncio.run(
            run_grpc_worker(
                server=server.server,
                tasks_url=tasks_url
                or os.environ.get("STEPFLOW_TASKS_URL", "localhost:7837"),
                queue_name=queue_name
                or os.environ.get("STEPFLOW_QUEUE_NAME", "python"),
                max_concurrent=max_concurrent
                or int(os.environ.get("STEPFLOW_MAX_CONCURRENT", "4")),
            )
        )

    except KeyboardInterrupt:
        click.echo("\nServer stopped", err=True)
    except Exception as e:
        click.echo(f"Server error: {e}", err=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
