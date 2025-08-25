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

"""Stepflow component server for Langflow integration."""

from typing import Dict, Any
from stepflow_py import StepflowStdioServer, StepflowContext


class CachedStepflowContext:
    """A wrapper around StepflowContext that returns cached blob data to avoid JSON-RPC deadlocks.

    This class prevents circular JSON-RPC dependencies by serving blob data from a local cache
    instead of making new context.get_blob() calls during component execution.
    """

    def __init__(self, original_context: StepflowContext, cached_blobs: Dict[str, Any]):
        """Initialize with original context and cached blob data."""
        self.original_context = original_context
        self.cached_blobs = cached_blobs

    async def get_blob(self, blob_id: str) -> Any:
        """Return cached blob data to avoid JSON-RPC circular dependencies."""
        if blob_id in self.cached_blobs:
            print(f"DEBUG: Returning cached blob data for {blob_id}")
            return self.cached_blobs[blob_id]
        else:
            # This should not happen if we cached all blobs correctly
            print(
                f"WARNING: Blob {blob_id} not found in cache, attempting direct fetch"
            )
            # Fall back to original context as last resort (may cause deadlock)
            return await self.original_context.get_blob(blob_id)

    async def put_blob(self, data: Any) -> str:
        """Delegate put_blob to original context (this won't cause deadlocks)."""
        return await self.original_context.put_blob(data)

    def __getattr__(self, name: str):
        """Delegate all other methods to the original context."""
        return getattr(self.original_context, name)


# Handle relative imports when run directly
try:
    from .udf_executor import UDFExecutor
except ImportError:
    import sys
    from pathlib import Path

    # Add parent directory to path for direct execution
    sys.path.insert(0, str(Path(__file__).parent.parent))
    from executor.udf_executor import UDFExecutor


class StepflowLangflowServer:
    """Stepflow component server with Langflow UDF execution capabilities."""

    def __init__(self):
        """Initialize the Langflow component server."""
        self.server = StepflowStdioServer()
        self.udf_executor = UDFExecutor()

        # Register components
        self._register_components()

    def _register_components(self) -> None:
        """Register all Langflow components."""
        # Register the main UDF executor component

        @self.server.component(name="udf_executor")
        async def udf_executor(
            input_data: Dict[str, Any], context: StepflowContext
        ) -> Dict[str, Any]:
            """Execute a Langflow UDF component with blob caching to avoid JSON-RPC deadlocks."""
            print("🔥 LANGFLOW SERVER: ENTERED UDF_EXECUTOR WITH BLOB CACHING!")
            with open("/tmp/udf_debug.log", "a") as f:
                f.write("🔥 LANGFLOW SERVER: ENTERED UDF_EXECUTOR WITH BLOB CACHING!\n")

            # CRITICAL FIX: Pre-cache all blob data to avoid context.get_blob() calls during execution
            # This prevents JSON-RPC circular dependencies that cause deadlocks

            print(f"DEBUG CACHE: Input data keys: {list(input_data.keys())}")
            for key, value in input_data.items():
                if isinstance(value, dict) and "$from" in value:
                    print(f"DEBUG CACHE: {key} = step reference: {value}")
                elif isinstance(value, str) and len(value) == 64:  # Likely blob ID
                    print(f"DEBUG CACHE: {key} = possible blob ID: {value}")
                else:
                    print(
                        f"DEBUG CACHE: {key} = {type(value).__name__}: {str(value)[:100]}"
                    )

            cached_blobs = {}

            # Identify all blob IDs that need to be cached
            blob_ids_to_cache = set()

            # Add main blob_id if present
            if "blob_id" in input_data:
                blob_ids_to_cache.add(input_data["blob_id"])

            # Add tool blob IDs (external inputs like tool_blob_X)
            for key, value in input_data.items():
                if key.startswith("tool_blob_") and isinstance(value, dict):
                    # Extract blob ID from reference structure
                    if "$from" in value and "step" in value["$from"]:
                        # This is a step reference - we can't resolve it here
                        continue
                    elif isinstance(value, str):
                        blob_ids_to_cache.add(value)
                    elif "blob_id" in value:
                        blob_ids_to_cache.add(value["blob_id"])

            # Add agent blob ID if present
            if "agent_blob" in input_data:
                agent_ref = input_data["agent_blob"]
                if isinstance(agent_ref, dict):
                    if "$from" in agent_ref and "step" in agent_ref["$from"]:
                        # This is a step reference - we can't resolve it here
                        pass
                    elif "blob_id" in agent_ref:
                        blob_ids_to_cache.add(agent_ref["blob_id"])
                elif isinstance(agent_ref, str):
                    blob_ids_to_cache.add(agent_ref)

            # Pre-fetch all blob data to avoid deadlocks
            for blob_id in blob_ids_to_cache:
                try:
                    print(f"DEBUG: Pre-caching blob {blob_id}")
                    blob_data = await context.get_blob(blob_id)
                    cached_blobs[blob_id] = blob_data
                    print(
                        f"DEBUG: Successfully cached blob {blob_id} with keys: {list(blob_data.keys()) if isinstance(blob_data, dict) else type(blob_data)}"
                    )
                except Exception as e:
                    print(f"DEBUG: Failed to cache blob {blob_id}: {e}")
                    # Continue - let the UDF executor handle the error

            # Create a modified context that returns cached data instead of making new calls
            cached_context = CachedStepflowContext(context, cached_blobs)

            return await self.udf_executor.execute(input_data, cached_context)

        # TODO: Register native component implementations
        # self.server.component(name="openai_chat", func=self._openai_chat)
        # self.server.component(name="chat_input", func=self._chat_input)

    def run(self) -> None:
        """Run the component server."""
        self.server.run()

    async def serve(self, host: str = "localhost", port: int = 8000) -> None:
        """Run the component server in HTTP mode.

        Args:
            host: Server host
            port: Server port
        """
        # This would require HTTP server implementation
        # For now, fallback to stdio
        self.run()


if __name__ == "__main__":
    """Run the Langflow component server."""
    server = StepflowLangflowServer()
    server.run()
