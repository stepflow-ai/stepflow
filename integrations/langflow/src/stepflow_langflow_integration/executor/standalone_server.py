#!/usr/bin/env python3
"""Standalone Stepflow component server for Langflow integration.

This script can be run directly and handles imports properly.
"""

import sys
import os
from pathlib import Path

# Add the package root to the path
package_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(package_root))

from typing import Dict, Any
from stepflow_py import StepflowStdioServer, StepflowContext
from stepflow_langflow_integration.executor.udf_executor import UDFExecutor

# Create server instance (following the exact pattern from stepflow_py/main.py)
server = StepflowStdioServer()

# Create UDF executor
udf_executor = UDFExecutor()

# Register the main UDF executor component at module level
@server.component(name="udf_executor")
async def udf_executor_component(input_data: Dict[str, Any], context: StepflowContext) -> Dict[str, Any]:
    """Execute a Langflow UDF component."""
    return await udf_executor.execute(input_data, context)

# TODO: Register native component implementations
# @server.component(name="openai_chat")
# async def openai_chat(input_data: Dict[str, Any], context: StepflowContext) -> Dict[str, Any]:
#     pass

def main():
    """Main entry point for the Langflow component server."""
    # Start the server - this handles all the asyncio setup correctly
    server.run()


if __name__ == "__main__":
    """Run the Langflow component server."""
    main()