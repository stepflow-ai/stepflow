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

from __future__ import annotations

import asyncio
import sys
from typing import Any, TypeVar
from uuid import uuid4

from stepflow_py.generated_flow import Flow
from stepflow_py.generated_protocol import (
    BatchDetails,
    BatchOutputInfo,
    BlobType,
    EvaluateFlowParams,
    EvaluateFlowResult,
    FlowResultFailed,
    FlowResultSkipped,
    FlowResultSuccess,
    GetBatchParams,
    GetBatchResult,
    GetBlobResult,
    GetFlowMetadataParams,
    GetFlowMetadataResult,
    Message,
    Method,
    MethodError,
    MethodSuccess,
    PutBlobParams,
    PutBlobResult,
    SubmitBatchParams,
    SubmitBatchResult,
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
        step_id: str | None = None,
        run_id: str | None = None,
        flow_id: str | None = None,
        attempt: int = 1,
    ):
        self._outgoing_queue = outgoing_queue
        self._message_decoder = message_decoder
        self._session_id = session_id
        self._step_id = step_id
        self._run_id = run_id
        self._flow_id = flow_id
        self._attempt = attempt

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

    async def put_blob(self, data: Any, blob_type: BlobType = BlobType.data) -> str:
        """Store JSON data as a blob and return its content-based ID.

        Args:
            data: The JSON-serializable data to store
            blob_type: The type of blob to store (flow or data)

        Returns:
            The blob ID (SHA-256 hash) for the stored data
        """
        params = PutBlobParams(data=data, blob_type=blob_type)
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

    @property
    def step_id(self) -> str | None:
        """Get the current step ID, or None if not available."""
        return self._step_id

    @property
    def run_id(self) -> str | None:
        """Get the current run ID, or None if not available."""
        return self._run_id

    @property
    def flow_id(self) -> str | None:
        """Get the current flow ID, or None if not available."""
        return self._flow_id

    @property
    def attempt(self) -> int:
        """Get the current attempt number (1-based, for retry logic)."""
        return self._attempt

    async def evaluate_flow(self, flow: Flow, input: Any) -> Any:
        """Evaluate a flow with the given input.

        Args:
            flow: The flow definition
            input: The input to provide to the flow

        Returns:
            The result value on success

        Raises:
            StepflowSkipped: If the flow execution was skipped
            StepflowFailed: If the flow execution failed with a business logic error
            Exception: For system/runtime errors
        """
        # Convert Flow object to dict for blob storage
        import msgspec

        flow_dict = msgspec.to_builtins(flow)

        # Store flow as a blob first
        flow_id = await self.put_blob(flow_dict, BlobType.flow)

        # Delegate to evaluate_flow_by_id for the actual evaluation
        return await self.evaluate_flow_by_id(flow_id, input)

    async def evaluate_flow_by_id(self, flow_id: str, input: Any) -> Any:
        """Evaluate a flow by its blob ID with the given input.

        Args:
            flow_id: The blob ID of the flow to evaluate
            input: The input to provide to the flow

        Returns:
            The result value on success

        Raises:
            StepflowSkipped: If the flow execution was skipped
            StepflowFailed: If the flow execution failed with a business logic error
            Exception: For system/runtime errors
        """
        from stepflow_py.exceptions import StepflowFailed, StepflowSkipped

        params = EvaluateFlowParams(
            flow_id=flow_id,
            input=input,
        )
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

    async def get_metadata(self, step_id: str | None = None) -> dict[str, Any]:
        """Get metadata for the current flow and optionally a specific step.

        Args:
            step_id: The ID of the step to get metadata for. If None, uses
                the current step ID. If neither step_id nor current step_id
                are available, only flow metadata is returned.

        Returns:
            A dictionary containing:
            - flow_metadata: Metadata for the current flow
            - step_metadata: Metadata for the specified step (if step_id
              provided and found)
        """
        # Use provided step_id, or fall back to current step_id, or None
        target_step_id = step_id or self._step_id

        assert self._flow_id is not None, "flow_id is not available in context"
        params = GetFlowMetadataParams(
            step_id=target_step_id,
            flow_id=self._flow_id,
        )
        response = await self._send_request(
            Method.flows_get_metadata, params, GetFlowMetadataResult
        )

        result = {"flow_metadata": response.flow_metadata}
        if response.step_metadata is not None:
            result["step_metadata"] = response.step_metadata

        return result

    async def submit_batch(
        self,
        flow: Flow,
        inputs: list[Any],
        max_concurrency: int | None = None,
    ) -> str:
        """Submit a batch of inputs for parallel execution and return the batch ID.

        Args:
            flow: The flow definition to execute
            inputs: List of inputs to process in parallel
            max_concurrency: Maximum number of concurrent executions (optional)

        Returns:
            The batch ID for tracking the batch execution
        """
        import msgspec

        # Convert Flow object to dict for blob storage
        flow_dict = msgspec.to_builtins(flow)

        # Store flow as a blob first
        flow_id = await self.put_blob(flow_dict, BlobType.flow)

        # Delegate to submit_batch_by_id
        return await self.submit_batch_by_id(flow_id, inputs, max_concurrency)

    async def submit_batch_by_id(
        self,
        flow_id: str,
        inputs: list[Any],
        max_concurrency: int | None = None,
    ) -> str:
        """Submit a batch of inputs for parallel execution using a flow ID.

        Args:
            flow_id: The blob ID of the flow to execute
            inputs: List of inputs to process in parallel
            max_concurrency: Maximum number of concurrent executions (optional)

        Returns:
            The batch ID for tracking the batch execution
        """
        params = SubmitBatchParams(
            flow_id=flow_id,
            inputs=inputs,
            max_concurrency=max_concurrency,
        )
        response = await self._send_request(
            Method.flows_submit_batch, params, SubmitBatchResult
        )
        return response.batch_id

    async def get_batch(
        self,
        batch_id: str,
        wait: bool = False,
        include_results: bool = False,
    ) -> tuple[BatchDetails, list[BatchOutputInfo] | None]:
        """Get batch status and optionally wait for completion and retrieve results.

        Args:
            batch_id: The ID of the batch to retrieve
            wait: If True, wait for batch completion before returning
            include_results: If True, include the run results in the response

        Returns:
            A tuple of (batch_details, outputs) where outputs is None if
            include_results is False
        """
        params = GetBatchParams(
            batch_id=batch_id,
            wait=wait,
            include_results=include_results,
        )
        response = await self._send_request(
            Method.flows_get_batch, params, GetBatchResult
        )
        return (response.details, response.outputs)

    async def evaluate_batch(
        self,
        flow: Flow,
        inputs: list[Any],
        max_concurrency: int | None = None,
    ) -> list[Any]:
        """Submit a batch, wait for completion, and return all results.

        This is a convenience method that combines submit_batch and get_batch
        with wait=True and include_results=True.

        Args:
            flow: The flow definition to execute
            inputs: List of inputs to process in parallel
            max_concurrency: Maximum number of concurrent executions (optional)

        Returns:
            List of results corresponding to each input, in the same order

        Raises:
            Exception: If any of the runs failed or were skipped
        """
        import msgspec

        # Convert Flow object to dict for blob storage
        flow_dict = msgspec.to_builtins(flow)

        # Store flow as a blob first
        flow_id = await self.put_blob(flow_dict, BlobType.flow)

        # Delegate to evaluate_batch_by_id
        return await self.evaluate_batch_by_id(flow_id, inputs, max_concurrency)

    async def evaluate_batch_by_id(
        self,
        flow_id: str,
        inputs: list[Any],
        max_concurrency: int | None = None,
    ) -> list[Any]:
        """Submit a batch by flow ID, wait for completion, and return all results.

        This is a convenience method that combines submit_batch_by_id and get_batch
        with wait=True and include_results=True.

        Args:
            flow_id: The blob ID of the flow to execute
            inputs: List of inputs to process in parallel
            max_concurrency: Maximum number of concurrent executions (optional)

        Returns:
            List of results corresponding to each input, in the same order

        Raises:
            Exception: If any of the runs failed or were skipped
        """
        from stepflow_py.exceptions import StepflowFailed, StepflowSkipped

        # Submit the batch
        batch_id = await self.submit_batch_by_id(flow_id, inputs, max_concurrency)

        # Wait for completion and get results
        _details, outputs = await self.get_batch(
            batch_id, wait=True, include_results=True
        )

        # Extract results from outputs
        assert outputs is not None, "include_results=True should return outputs"

        results = []
        for output_info in outputs:
            if output_info.result is None:
                raise Exception(
                    f"Run at index {output_info.batch_input_index} has no "
                    f"result (status: {output_info.status})"
                )

            flow_result = output_info.result

            # Check the outcome and either append the result or raise exception
            if isinstance(flow_result, FlowResultSuccess):
                results.append(flow_result.result)
            elif isinstance(flow_result, FlowResultSkipped):
                raise StepflowSkipped(
                    f"Run at index {output_info.batch_input_index} was skipped"
                )
            elif isinstance(flow_result, FlowResultFailed):
                error = flow_result.error
                raise StepflowFailed(
                    error_code=error.code,
                    message=(
                        f"Run at index {output_info.batch_input_index} "
                        f"failed: {error.message}"
                    ),
                    data=error.data,
                )
            else:
                raise Exception(f"Unexpected flow result type: {type(flow_result)}")

        return results

    def log(self, message):
        """Log a message."""
        print(f"PYTHON: {message}", file=sys.stderr)
