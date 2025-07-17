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
from typing import Any
from uuid import uuid4

from stepflow_sdk.exceptions import StepflowRuntimeError
from stepflow_sdk.generated_protocol import (
    EvaluateFlowResult,
    Failed,
    Flow,
    GetBlobResult,
    Message,
    MethodError,
    MethodSuccess,
    PutBlobResult,
    Skipped,
    Success,
)
from stepflow_sdk.message_decoder import MessageDecoder

"""
Context API for stepflow components to interact with the runtime.
"""


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

    async def _send_request[T](
        self, method: str, params: Any, result_type: type[T]
    ) -> T:
        """Send a request to the stepflow runtime and wait for response."""
        request_id = str(uuid4())
        request = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        }

        # Create future for response
        future: asyncio.Future[Message] = asyncio.Future()

        # Register the pending request with the message decoder
        self._message_decoder.register_request(request_id, result_type, future)

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
        response = await self._send_request("blobs/put", params, PutBlobResult)
        return response.blob_id

    async def get_blob(self, blob_id: str) -> Any:
        """Retrieve JSON data by blob ID.

        Args:
            blob_id: The blob ID to retrieve

        Returns:
            The JSON data associated with the blob ID
        """
        params = {"blob_id": blob_id}
        response = await self._send_request("blobs/get", params, GetBlobResult)
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
        from stepflow_sdk.exceptions import StepflowFailed, StepflowSkipped

        # Convert Flow object to dict if needed
        if isinstance(flow, Flow):
            import msgspec

            flow_dict = msgspec.to_builtins(flow)
        else:
            flow_dict = flow

        params = {"flow": flow_dict, "input": input}
        # Use raw dict response to avoid union deserialization issues
        request_id = str(uuid4())
        request = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "flows/evaluate",
            "params": params,
        }

        # Create future for response
        future: asyncio.Future[Message] = asyncio.Future()

        # Register the pending request with the message decoder
        self._message_decoder.register_request(request_id, dict, future)

        # Send request via queue
        await self._outgoing_queue.put(request)

        # Wait for response - the MessageDecoder will resolve this future
        response_message = await future

        if isinstance(response_message, MethodError):
            raise Exception(f"Flow evaluation failed: {response_message.error}")
        elif isinstance(response_message, MethodSuccess):
            if isinstance(response_message.result, EvaluateFlowResult):
                flow_result = response_message.result.result
            else:
                raise StepflowRuntimeError(
                    f"Expected EvaluateFlowResult, got {response_message.result}"
                )
        else:
            raise StepflowRuntimeError(f"Uenexpected response {response_message}")

        # Check the outcome and either return the result or raise appropriate exception
        if isinstance(flow_result, dict):
            # Handle dictionary-based flow results
            outcome = flow_result.get("outcome")
            if outcome == "success":
                return flow_result.get("result")
            elif outcome == "skipped":
                raise StepflowSkipped("Flow execution was skipped")
            elif outcome == "failed":
                error = flow_result.get("error", {})
                raise StepflowFailed(
                    error_code=error.get("code", 500),
                    message=error.get("message", "Flow execution failed"),
                    data=error.get("data"),
                )
            else:
                raise Exception(f"Unexpected flow outcome: {outcome}")
        elif isinstance(flow_result, Success):
            return flow_result.result
        elif isinstance(flow_result, Skipped):
            raise StepflowSkipped("Flow execution was skipped")
        elif isinstance(flow_result, Failed):
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
