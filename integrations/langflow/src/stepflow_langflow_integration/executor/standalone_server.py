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

"""Standalone Stepflow component server for Langflow integration.

This script can be run directly and handles imports properly.
"""

import sys
from pathlib import Path
from typing import Any

# Add the package root to the path before importing project modules
package_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(package_root))

from stepflow_py import StepflowContext, StepflowStdioServer

from stepflow_langflow_integration.executor.udf_executor import UDFExecutor

# Create server instance (following the exact pattern from stepflow_py/main.py)
server = StepflowStdioServer()

# Create UDF executor
udf_executor = UDFExecutor()


# Register the main UDF executor component at module level
@server.component(name="udf_executor")
async def udf_executor_component(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Execute a Langflow UDF component."""
    return await udf_executor.execute(input_data, context)


# All Langflow components now route through the UDF executor for real execution
# No hardcoded component implementations - everything uses real Langflow code


def main():
    """Main entry point for the Langflow component server."""
    import nest_asyncio  # type: ignore

    nest_asyncio.apply()
    # Start the server - this handles all the asyncio setup correctly
    server.run()


if __name__ == "__main__":
    """Run the Langflow component server."""
    main()
