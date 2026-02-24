"""Main CLI entry point for Docling step worker."""

from __future__ import annotations

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
@click.option("--host", default="localhost", help="Server host")
@click.option("--port", default=0, help="Server port (0 for auto-assign)")
def serve(host: str, port: int) -> None:
    """Start the Docling step worker server.

    The server runs in HTTP mode and prints the port as JSON to stdout
    for the stepflow orchestrator to discover.

    Environment variables:
        STEPFLOW_LOG_LEVEL: Log level (default: INFO)
        STEPFLOW_LOG_DESTINATION: Log destination (stderr, file, otlp)
    """
    from stepflow_py.worker.observability import setup_observability

    setup_observability()

    try:
        click.echo("Starting Docling step worker server...", err=True)

        server = DoclingStepWorkerServer()
        server.run(host=host, port=port)

    except KeyboardInterrupt:
        click.echo("\nServer stopped", err=True)
    except Exception as e:
        click.echo(f"Server error: {e}", err=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
