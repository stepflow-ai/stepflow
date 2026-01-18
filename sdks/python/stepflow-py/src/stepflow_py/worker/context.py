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
from typing import Any, TypeVar
from uuid import uuid4

from stepflow_py.api.models import Flow
from stepflow_py.worker.generated_protocol import (
    BlobType,
    FlowResultFailed,
    FlowResultSuccess,
    GetBlobResult,
    GetRunProtocolParams,
    Message,
    Method,
    MethodError,
    MethodSuccess,
    ObservabilityContext,
    PutBlobParams,
    PutBlobResult,
    ResultOrder,
    RunStatusProtocol,
    SubmitRunProtocolParams,
)
from stepflow_py.worker.message_decoder import MessageDecoder

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
        observability: ObservabilityContext | None = None,
    ):
        self._outgoing_queue = outgoing_queue
        self._message_decoder = message_decoder
        self._session_id = session_id
        self._step_id = step_id
        self._run_id = run_id
        self._flow_id = flow_id
        self._attempt = attempt
        self._observability = observability

    def current_observability_context(self) -> ObservabilityContext | None:
        """Get the current observability context for bidirectional requests.

        This captures the current OpenTelemetry span context and combines it
        with the execution context (run_id, flow_id, step_id) to create proper
        trace propagation for bidirectional calls.

        Returns:
            ObservabilityContext with current span as parent, or None if
            no span is active.
        """
        from stepflow_py.worker.observability import get_current_observability_context

        return get_current_observability_context(
            run_id=self._run_id,
            flow_id=self._flow_id,
            step_id=self._step_id,
        )

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
        from stepflow_py.worker.observability import get_tracer

        tracer = get_tracer(__name__)
        with tracer.start_as_current_span(
            "put_blob",
            attributes={
                "blob_type": blob_type.value,
            },
        ):
            params = PutBlobParams(
                data=data,
                blob_type=blob_type,
                observability=self.current_observability_context(),
            )
            response = await self._send_request(Method.blobs_put, params, PutBlobResult)
            return response.blob_id

    async def get_blob(self, blob_id: str) -> Any:
        """Retrieve JSON data by blob ID.

        Args:
            blob_id: The blob ID to retrieve

        Returns:
            The JSON data associated with the blob ID
        """
        from stepflow_py.worker.generated_protocol import GetBlobParams
        from stepflow_py.worker.observability import get_tracer

        tracer = get_tracer(__name__)
        with tracer.start_as_current_span(
            "get_blob",
            attributes={
                "blob_id": blob_id,
            },
        ):
            params = GetBlobParams(
                blob_id=blob_id, observability=self.current_observability_context()
            )
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

    async def evaluate_flow(self, flow: Flow, input: Any, overrides: Any = None) -> Any:
        """Evaluate a flow with the given input.

        Args:
            flow: The flow definition
            input: The input to provide to the flow
            overrides: Optional workflow overrides to apply before execution

        Returns:
            The result value on success

        Raises:
            StepflowFailed: If the flow execution failed with a business logic error
            Exception: For system/runtime errors
        """
        # Convert Flow object to dict for blob storage
        flow_dict = flow.model_dump()

        # Store flow as a blob first
        flow_id = await self.put_blob(flow_dict, BlobType.flow)

        # Delegate to evaluate_flow_by_id for the actual evaluation
        return await self.evaluate_flow_by_id(flow_id, input, overrides=overrides)

    async def evaluate_flow_by_id(
        self, flow_id: str, input: Any, overrides: Any = None
    ) -> Any:
        """Evaluate a flow by its blob ID with the given input.

        Args:
            flow_id: The blob ID of the flow to evaluate
            input: The input to provide to the flow
            overrides: Optional workflow overrides to apply before execution

        Returns:
            The result value on success

        Raises:
            StepflowFailed: If the flow execution failed with a business logic error
            Exception: For system/runtime errors
        """
        # Use the new unified runs API with a single input
        results = await self.evaluate_run_by_id(flow_id, [input], overrides=overrides)
        # Return the single result
        return results[0]

    # =========================================================================
    # Unified Runs API
    # =========================================================================

    async def submit_run(
        self,
        flow: Flow,
        inputs: list[Any],
        wait: bool = False,
        max_concurrency: int | None = None,
        overrides: Any = None,
    ) -> RunStatusProtocol:
        """Submit a run (1 or N items) for execution.

        Args:
            flow: The flow definition to execute
            inputs: List of inputs to process (can be a single-item list)
            wait: If True, wait for completion before returning
            max_concurrency: Maximum number of concurrent executions (optional)
            overrides: Optional workflow overrides to apply

        Returns:
            RunStatusProtocol with run status and optionally results if wait=True
        """
        # Convert Flow object to dict for blob storage
        flow_dict = flow.model_dump()

        # Store flow as a blob first
        flow_id = await self.put_blob(flow_dict, BlobType.flow)

        # Delegate to submit_run_by_id
        return await self.submit_run_by_id(
            flow_id, inputs, wait, max_concurrency, overrides=overrides
        )

    async def submit_run_by_id(
        self,
        flow_id: str,
        inputs: list[Any],
        wait: bool = False,
        max_concurrency: int | None = None,
        overrides: Any = None,
    ) -> RunStatusProtocol:
        """Submit a run by flow ID.

        Args:
            flow_id: The blob ID of the flow to execute
            inputs: List of inputs to process
            wait: If True, wait for completion before returning
            max_concurrency: Maximum number of concurrent executions (optional)
            overrides: Optional workflow overrides to apply

        Returns:
            RunStatusProtocol with run status and optionally results if wait=True
        """
        from stepflow_py.worker.observability import get_tracer

        tracer = get_tracer(__name__)
        attributes: dict[str, str | int | bool] = {
            "flow_id": flow_id,
            "item_count": len(inputs),
            "wait": wait,
        }
        if max_concurrency is not None:
            attributes["max_concurrency"] = max_concurrency

        with tracer.start_as_current_span("submit_run", attributes=attributes):
            params = SubmitRunProtocolParams(
                flowId=flow_id,
                inputs=inputs,
                wait=wait,
                maxConcurrency=max_concurrency,
                overrides=overrides,
                observability=self.current_observability_context(),
            )
            response = await self._send_request(
                Method.runs_submit, params, RunStatusProtocol
            )
            return response

    async def get_run(
        self,
        run_id: str,
        wait: bool = False,
        include_results: bool = False,
        result_order: ResultOrder = ResultOrder.by_index,
    ) -> RunStatusProtocol:
        """Get run status and optionally wait for completion and retrieve results.

        Args:
            run_id: The ID of the run to retrieve
            wait: If True, wait for run completion before returning
            include_results: If True, include the item results in the response
            result_order: Order of results (byIndex or byCompletion)

        Returns:
            RunStatusProtocol with run status and optionally results
        """
        from stepflow_py.worker.observability import get_tracer

        tracer = get_tracer(__name__)
        with tracer.start_as_current_span(
            "get_run",
            attributes={
                "run_id": run_id,
                "wait": wait,
                "include_results": include_results,
            },
        ):
            params = GetRunProtocolParams(
                runId=run_id,
                wait=wait,
                includeResults=include_results,
                resultOrder=result_order,
                observability=self.current_observability_context(),
            )
            response = await self._send_request(
                Method.runs_get, params, RunStatusProtocol
            )
            return response

    async def evaluate_run(
        self,
        flow: Flow,
        inputs: list[Any],
        max_concurrency: int | None = None,
        overrides: Any = None,
    ) -> list[Any]:
        """Submit a run, wait for completion, and return all results.

        This is a convenience method that combines submit_run and get_run
        with wait=True and include_results=True.

        Args:
            flow: The flow definition to execute
            inputs: List of inputs to process
            max_concurrency: Maximum number of concurrent executions (optional)
            overrides: Optional workflow overrides to apply

        Returns:
            List of results corresponding to each input, in the same order

        Raises:
            StepflowFailed: If any of the runs failed
        """
        # Convert Flow object to dict for blob storage
        flow_dict = flow.model_dump(by_alias=True, exclude_unset=True)

        # Store flow as a blob first
        flow_id = await self.put_blob(flow_dict, BlobType.flow)

        # Delegate to evaluate_run_by_id
        return await self.evaluate_run_by_id(
            flow_id, inputs, max_concurrency, overrides=overrides
        )

    async def evaluate_run_by_id(
        self,
        flow_id: str,
        inputs: list[Any],
        max_concurrency: int | None = None,
        overrides: Any = None,
    ) -> list[Any]:
        """Submit a run by flow ID, wait for completion, and return all results.

        This is a convenience method that combines submit_run_by_id and get_run
        with wait=True and include_results=True.

        Args:
            flow_id: The blob ID of the flow to execute
            inputs: List of inputs to process
            max_concurrency: Maximum number of concurrent executions (optional)
            overrides: Optional workflow overrides to apply

        Returns:
            List of results corresponding to each input, in the same order

        Raises:
            StepflowFailed: If any of the runs failed
        """
        from stepflow_py.worker.exceptions import StepflowFailed

        # Submit and wait for completion with results
        run_status = await self.submit_run_by_id(
            flow_id,
            inputs,
            wait=True,
            max_concurrency=max_concurrency,
            overrides=overrides,
        )

        # Extract results
        if run_status.results is None:
            raise Exception("Expected results in response when wait=True")

        results = []
        for item_result in run_status.results:
            if item_result.result is None:
                raise Exception(
                    f"Item at index {item_result.itemIndex} has no "
                    f"result (status: {item_result.status})"
                )

            flow_result = item_result.result

            # Check the outcome and either append the result or raise exception
            if isinstance(flow_result, FlowResultSuccess):
                results.append(flow_result.result)
            elif isinstance(flow_result, FlowResultFailed):
                error = flow_result.error
                raise StepflowFailed(
                    error_code=error.code,
                    message=(
                        f"Item at index {item_result.itemIndex} failed: {error.message}"
                    ),
                    data=error.data,
                )
            else:
                raise Exception(f"Unexpected flow result type: {type(flow_result)}")

        return results
