#!/usr/bin/env python3
"""
Simple echo server for testing HTTP protocol
"""
import argparse
import asyncio
import json
import sys
from typing import Any, Dict

try:
    from stepflow_sdk.server import StepflowServer, StepflowStdioServer
    from stepflow_sdk.http_server import StepflowHttpServer
    import msgspec
except ImportError:
    print("Error: This test requires the Python SDK", file=sys.stderr)
    print("Please install with: pip install stepflow-sdk[http]", file=sys.stderr)
    sys.exit(1)


class EchoInput(msgspec.Struct):
    message: str


class EchoOutput(msgspec.Struct):
    echo: str


def echo_component(input: EchoInput) -> EchoOutput:
    """Simple echo component that returns the input message"""
    return EchoOutput(echo=f"Echo: {input.message}")


async def main():
    parser = argparse.ArgumentParser(description="Echo Test Server")
    parser.add_argument("--http", action="store_true", help="Run in HTTP mode")
    parser.add_argument("--port", type=int, default=8080, help="HTTP port")
    parser.add_argument("--host", type=str, default="127.0.0.1", help="HTTP host")
    
    args = parser.parse_args()
    
    # Create core server instance
    core_server = StepflowServer(default_protocol_prefix="test")
    
    # Register the echo component
    core_server.component(echo_component, name="echo", description="Echo component for testing")
    
    if args.http:
        # Create HTTP server
        http_server = StepflowHttpServer(core_server, host=args.host, port=args.port)
        print(f"Starting HTTP echo server on {args.host}:{args.port}", file=sys.stderr)
        await http_server.run()
    else:
        # Create STDIO server wrapper
        stdio_server = StepflowStdioServer(core_server)
        print("Starting stdio echo server", file=sys.stderr)
        stdio_server.run()


if __name__ == "__main__":
    asyncio.run(main())