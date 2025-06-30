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

"""
Context API for stepflow components to interact with the runtime.
"""
import json
import sys
import asyncio
from typing import Any, Dict
from uuid import uuid4, UUID
import msgspec


class StepflowContext:
    """
    Context for stepflow components to make calls back to the runtime.
    
    This allows components to store/retrieve blobs and perform other 
    runtime operations through bidirectional communication.
    """
    
    def __init__(self, outgoing_queue: asyncio.Queue, pending_requests: Dict[UUID, asyncio.Future]):
        self._outgoing_queue = outgoing_queue
        self._pending_requests = pending_requests
        
    async def _send_request(self, method: str, params: Any) -> Any:
        """Send a request to the stepflow runtime and wait for response."""
        request_id = uuid4()
        request = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params
        }
        
        # Create future for response
        future = asyncio.Future()
        self._pending_requests[request_id] = future
        
        # Send request via queue
        await self._outgoing_queue.put(request)
        
        # Wait for response
        try:
            response = await future
            return response
        finally:
            # Clean up
            self._pending_requests.pop(request_id, None)
    
    
    async def put_blob(self, data: Any) -> str:
        """
        Store JSON data as a blob and return its content-based ID.
        
        Args:
            data: The JSON-serializable data to store
            
        Returns:
            The blob ID (SHA-256 hash) for the stored data
        """
        params = {"data": data}
        response = await self._send_request("put_blob", params)
        return response["blob_id"]
    
    async def get_blob(self, blob_id: str) -> Any:
        """
        Retrieve JSON data by blob ID.
        
        Args:
            blob_id: The blob ID to retrieve
            
        Returns:
            The JSON data associated with the blob ID
        """
        params = {"blob_id": blob_id}
        response = await self._send_request("get_blob", params)
        return response["data"]
    
    def get_sync_proxy(self):
        """
        Get a synchronous proxy object for use in non-async contexts.
        This uses the current event loop to run async operations.
        """
        return SyncBlobProxy(self)
    
    def log(self, message):
        """
        Log a message.
        """
        print(f"PYTHON: {message}", file=sys.stderr)


class SyncBlobProxy:
    """
    Synchronous proxy for blob operations that can be used in custom component code.
    """
    
    def __init__(self, context: StepflowContext):
        self._context = context
        
    def put_blob(self, data: Any) -> str:
        """
        Store JSON data as a blob synchronously.
        
        Args:
            data: The JSON-serializable data to store
            
        Returns:
            The blob ID (SHA-256 hash) for the stored data
        """
        import asyncio
        # Get the current event loop
        loop = asyncio.get_event_loop()
        if loop.is_running():
            # If we're already in an async context, create a task
            # This is a bit of a hack, but we need to run the task to completion
            # In a real implementation, this would need to be handled differently
            import concurrent.futures
            with concurrent.futures.ThreadPoolExecutor() as executor:
                future = executor.submit(asyncio.run, self._context.put_blob(data))
                return future.result()
        else:
            return asyncio.run(self._context.put_blob(data))
    
    def get_blob(self, blob_id: str) -> Any:
        """
        Retrieve JSON data by blob ID synchronously.
        
        Args:
            blob_id: The blob ID to retrieve
            
        Returns:
            The JSON data associated with the blob ID
        """
        import asyncio
        # Get the current event loop
        loop = asyncio.get_event_loop()
        if loop.is_running():
            # If we're already in an async context, create a task
            import concurrent.futures
            with concurrent.futures.ThreadPoolExecutor() as executor:
                future = executor.submit(asyncio.run, self._context.get_blob(blob_id))
                return future.result()
        else:
            return asyncio.run(self._context.get_blob(blob_id))