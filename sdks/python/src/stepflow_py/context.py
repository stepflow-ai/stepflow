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

from __future__ import annotations

import asyncio
import sys
from typing import Any, TypeVar
from uuid import uuid4

from stepflow_py.generated_protocol import (
    EvaluateFlowResult,
    Flow,
    FlowResultFailed,
    FlowResultSkipped,
    FlowResultSuccess,
    GetBlobResult,
    Message,
    Method,
    MethodError,
    MethodSuccess,
    PutBlobResult,
)
from stepflow_py.message_decoder import MessageDecoder

"""
Context API for stepflow components to interact with the runtime.
"""

T = TypeVar("T")


class StepflowContext:
    """Context for stepflow components to make calls back to the runtime.

    This allows components to store/retrieve blobs and perform other
    runtime operations through bidirectional communication.
    """

    def __init__(
        self,
        outgoing_queue: asyncio.Queue,
        message_decoder: MessageDecoder[asyncio.Future[Message]],
        session_id: str | None = None,
    ):
        self._outgoing_queue = outgoing_queue
        self._message_decoder = message_decoder
        self._session_id = session_id

    async def _send_request(
        self, method: Method, params: Any, result_type: type[T]
    ) -> T:
        """Send a request to the stepflow runtime and wait for response."""
        request_id = str(uuid4())
        request = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method.value,
            "params": params,
        }

        # Create future for response
        future: asyncio.Future[Message] = asyncio.Future()

        # Register pending request with message decoder using method registration
        self._message_decoder.register_request_for_method(request_id, method, future)

        # Send request via queue
        await self._outgoing_queue.put(request)

        # Wait for response - the MessageDecoder will resolve this future
        # when the response is received
        response_message = await future

        # Extract the result from the response message
        if isinstance(response_message, MethodSuccess):
            result = response_message.result
            assert isinstance(result, result_type), (
                f"Expected {result_type}, got {type(result)}"
            )
            return result
        elif isinstance(response_message, MethodError):
            # Handle error case
            raise Exception(f"Request failed: {response_message.error}")
        else:
            raise Exception(
                f"Unexpected response type: {type(response_message)} {response_message}"
            )

    async def put_blob(self, data: Any) -> str:
        """Store JSON data as a blob and return its content-based ID.

        Args:
            data: The JSON-serializable data to store

        Returns:
            The blob ID (SHA-256 hash) for the stored data
        """
        params = {"data": data}
        response = await self._send_request(Method.blobs_put, params, PutBlobResult)
        return response.blob_id

    async def get_blob(self, blob_id: str) -> Any:
        """Retrieve JSON data by blob ID.

        Args:
            blob_id: The blob ID to retrieve

        Returns:
            The JSON data associated with the blob ID
        """
        params = {"blob_id": blob_id}
        response = await self._send_request(Method.blobs_get, params, GetBlobResult)
        return response.data

    @property
    def session_id(self) -> str | None:
        """Get the session ID for HTTP mode, or None for STDIO mode."""
        return self._session_id

    async def evaluate_flow(self, flow: Flow | dict, input: Any) -> Any:
        """Evaluate a flow with the given input.

        Args:
            flow: The flow definition (as a Flow object or dictionary)
            input: The input to provide to the flow

        Returns:
            The result value on success

        Raises:
            StepflowSkipped: If the flow execution was skipped
            StepflowFailed: If the flow execution failed with a business logic error
            Exception: For system/runtime errors
        """
        from stepflow_py.exceptions import StepflowFailed, StepflowSkipped

        # Convert Flow object to dict if needed
        if isinstance(flow, Flow):
            import msgspec

            flow_dict = msgspec.to_builtins(flow)
        else:
            flow_dict = flow

        params = {"flow": flow_dict, "input": input}

        evaluate_result = await self._send_request(
            Method.flows_evaluate, params, EvaluateFlowResult
        )
        flow_result = evaluate_result.result

        # Check the outcome and either return the result or raise appropriate exception
        if isinstance(flow_result, FlowResultSuccess):
            return flow_result.result
        elif isinstance(flow_result, FlowResultSkipped):
            raise StepflowSkipped("Flow execution was skipped")
        elif isinstance(flow_result, FlowResultFailed):
            error = flow_result.error
            raise StepflowFailed(
                error_code=error.code,
                message=error.message,
                data=error.data,
            )
        else:
            raise Exception(f"Unexpected flow result type: {type(flow_result)}")

    def log(self, message):
        """Log a message."""
        print(f"PYTHON: {message}", file=sys.stderr)
