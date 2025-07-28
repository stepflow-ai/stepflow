# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

import argparse
import asyncio

from stepflow_py.stdio_server import StepflowStdioServer
from stepflow_py.udf import udf

# Create server instance
server = StepflowStdioServer()

# Register the UDF component
server.component(udf)


def main():
    parser = argparse.ArgumentParser(description="StepFlow Python SDK Server")
    parser.add_argument("--http", action="store_true", help="Run in HTTP mode")
    parser.add_argument(
        "--port", type=int, default=8080, help="HTTP port (default: 8080)"
    )
    parser.add_argument(
        "--host",
        type=str,
        default="localhost",
        help="HTTP host (default: localhost)",
    )

    args = parser.parse_args()

    if args.http:
        # Import HTTP server here to avoid import if not needed
        from stepflow_py.http_server import StepflowHttpServer

        # Create HTTP server wrapping the stdio server
        http_server = StepflowHttpServer(server._server, host=args.host, port=args.port)

        # Start HTTP server
        asyncio.run(http_server.run())
    else:
        # Start the stdio server
        server.run()


if __name__ == "__main__":
    main()
