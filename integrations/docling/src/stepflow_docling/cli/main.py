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

"""Main CLI entry point for Docling integration."""

from __future__ import annotations

import asyncio
import json
import sys

import click
from dotenv import load_dotenv

from ..server.docling_server import StepflowDoclingServer


@click.group()
@click.version_option()
def main():
    """Stepflow Docling Integration CLI."""
    # Load environment variables from .env file if it exists
    load_dotenv()
    pass


@main.command()
@click.option("--host", default="localhost", help="Server host")
@click.option("--port", default=0, help="Server port (0 for auto-assign)")
@click.option(
    "--docling-url",
    envvar="DOCLING_SERVE_URL",
    default="http://localhost:5001",
    help="URL of the docling-serve instance",
)
@click.option(
    "--api-key",
    envvar="DOCLING_SERVE_API_KEY",
    default=None,
    help="API key for docling-serve authentication",
)
def serve(
    host: str,
    port: int,
    docling_url: str,
    api_key: str | None,
):
    """Start the Docling component server.

    The server runs in HTTP mode and prints the port as JSON to stdout
    for the stepflow orchestrator to discover.

    Environment variables:
        DOCLING_SERVE_URL: URL of the docling-serve instance
        DOCLING_SERVE_API_KEY: Optional API key for authentication
    """
    try:
        click.echo("Starting Docling component server...", err=True)
        click.echo(f"  docling-serve URL: {docling_url}", err=True)

        server = StepflowDoclingServer(
            docling_serve_url=docling_url,
            api_key=api_key,
        )
        server.run(host=host, port=port)

    except KeyboardInterrupt:
        click.echo("\nServer stopped", err=True)
    except Exception as e:
        click.echo(f"Server error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.option(
    "--docling-url",
    envvar="DOCLING_SERVE_URL",
    default="http://localhost:5001",
    help="URL of the docling-serve instance",
)
@click.option(
    "--timeout",
    default=30,
    help="Health check timeout in seconds",
)
def health(docling_url: str, timeout: int):
    """Check health of the docling-serve instance.

    Returns exit code 0 if healthy, 1 otherwise.
    """
    from ..client.docling_client import DoclingServeClient

    async def check_health():
        client = DoclingServeClient(base_url=docling_url, timeout=float(timeout))
        try:
            async with client:
                result = await client.health()
                click.echo(json.dumps(result, indent=2))
                return True
        except Exception as e:
            click.echo(f"Health check failed: {e}", err=True)
            return False

    try:
        is_healthy = asyncio.run(check_health())
        sys.exit(0 if is_healthy else 1)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.argument("url", type=str)
@click.option(
    "--docling-url",
    envvar="DOCLING_SERVE_URL",
    default="http://localhost:5001",
    help="URL of the docling-serve instance",
)
@click.option(
    "--output-format",
    type=click.Choice(["md", "json", "html", "text"]),
    default="md",
    help="Output format for the converted document",
)
@click.option(
    "--ocr/--no-ocr",
    default=True,
    help="Enable OCR processing",
)
@click.option(
    "--ocr-engine",
    type=click.Choice(["easyocr", "tesseract", "rapidocr"]),
    default="easyocr",
    help="OCR engine to use",
)
def convert(
    url: str,
    docling_url: str,
    output_format: str,
    ocr: bool,
    ocr_engine: str,
):
    """Convert a document from a URL using docling-serve.

    URL: The URL of the document to convert (PDF, Word, etc.)

    Example:
        stepflow-docling convert https://arxiv.org/pdf/2408.09869.pdf
    """
    from ..client.docling_client import ConversionOptions, DoclingServeClient, OcrEngine

    async def do_convert():
        client = DoclingServeClient(base_url=docling_url)

        # Build options
        engine_map = {
            "easyocr": OcrEngine.EASYOCR,
            "tesseract": OcrEngine.TESSERACT,
            "rapidocr": OcrEngine.RAPIDOCR,
        }

        options = ConversionOptions(
            to_formats=[output_format],
            do_ocr=ocr,
            ocr_engine=engine_map.get(ocr_engine),
        )

        try:
            async with client:
                click.echo(f"Converting {url}...", err=True)
                result = await client.convert_from_url(url, options)

                if result.error:
                    click.echo(f"Conversion error: {result.error}", err=True)
                    return False

                # Output the converted content
                if result.document:
                    content = result.document.get("md_content") or result.document.get(
                        "content", ""
                    )
                    click.echo(content)

                click.echo(
                    f"\nConversion completed in {result.processing_time:.2f}s",
                    err=True,
                )
                return True

        except Exception as e:
            click.echo(f"Conversion failed: {e}", err=True)
            return False

    try:
        success = asyncio.run(do_convert())
        sys.exit(0 if success else 1)
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
