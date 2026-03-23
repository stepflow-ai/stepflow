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

"""Context API for stepflow components to interact with the runtime.

StepflowContext is the base class that defines the interface available
to components. Concrete implementations (GrpcContext) provide the
actual transport-specific logic.
"""

from __future__ import annotations

import uuid
from typing import Any

import msgspec

from stepflow_py.worker.blob_ref import BLOB_TYPE_DATA, BLOB_TYPE_FLOW
from stepflow_py.worker.generated_flow import Flow


class StepflowContext:
    """Context for stepflow components to make calls back to the runtime.

    This allows components to store/retrieve blobs and perform other
    runtime operations. Concrete subclasses (GrpcContext) provide the
    actual transport implementation.
    """

    def __init__(
        self,
        *,
        step_id: str | None = None,
        run_id: str | None = None,
        flow_id: str | None = None,
        attempt: int = 1,
        blob_api_url: str | None = None,
    ):
        self._step_id = step_id
        self._run_id = run_id
        self._flow_id = flow_id
        self._attempt = attempt
        self._blob_api_url = blob_api_url
        self._next_subflow_key: int = 0

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
        """The execution attempt number (1-based).

        A monotonically increasing counter that increments on every
        re-execution of this step, regardless of the reason:

        - **Transport error**: The subprocess crashed or a network failure
          occurred. Retried up to the orchestrator's ``retry.transportMaxRetries``.
        - **Component error**: The component returned an error and the step
          has ``onError: { action: retry }``. Retried up to ``maxRetries``.
        - **Orchestrator recovery**: The orchestrator crashed and is
          re-executing tasks that were in-flight.

        Components can use this to implement idempotency guards or
        progressive fallback strategies.
        """
        return self._attempt

    async def put_blob(self, data: Any, blob_type: str = BLOB_TYPE_DATA) -> str:
        """Store JSON data as a blob and return its content-based ID.

        Args:
            data: The JSON-serializable data to store
            blob_type: The type of blob to store (flow or data)

        Returns:
            The blob ID (SHA-256 hash) for the stored data
        """
        raise NotImplementedError("Subclasses must implement put_blob")

    async def get_blob(self, blob_id: str) -> Any:
        """Retrieve JSON data by blob ID.

        Args:
            blob_id: The blob ID to retrieve

        Returns:
            The JSON data associated with the blob ID
        """
        raise NotImplementedError("Subclasses must implement get_blob")

    def _generate_subflow_key(self) -> str:
        """Generate a deterministic subflow key.

        Uses UUID v5 with the run_id as namespace and an incrementing
        counter as the name. This ensures that when a step re-executes
        after recovery, the same sequence of calls produces the same
        keys, allowing the executor to match submissions to their
        pre-crash counterparts.
        """
        counter = self._next_subflow_key
        self._next_subflow_key += 1
        namespace = uuid.UUID(self._run_id) if self._run_id else uuid.NAMESPACE_DNS
        return str(uuid.uuid5(namespace, str(counter)))

    async def evaluate_flow(
        self,
        flow: Flow,
        input: Any,
        overrides: Any = None,
        subflow_key: uuid.UUID | str | None = None,
    ) -> Any:
        """Evaluate a flow with the given input.

        Args:
            flow: The flow definition
            input: The input to provide to the flow
            overrides: Optional workflow overrides to apply before execution
            subflow_key: Optional key for subflow deduplication during recovery.
                If not provided, a deterministic key is auto-generated.

        Returns:
            The result value on success

        Raises:
            StepflowFailed: If the flow execution failed with a business logic error
            Exception: For system/runtime errors
        """
        # Convert Flow object to dict for blob storage
        flow_dict = msgspec.to_builtins(flow)

        # Store flow as a blob first
        flow_id = await self.put_blob(flow_dict, BLOB_TYPE_FLOW)

        # Delegate to evaluate_flow_by_id for the actual evaluation
        return await self.evaluate_flow_by_id(
            flow_id, input, overrides=overrides, subflow_key=subflow_key
        )

    async def evaluate_flow_by_id(
        self,
        flow_id: str,
        input: Any,
        overrides: Any = None,
        subflow_key: uuid.UUID | str | None = None,
    ) -> Any:
        """Evaluate a flow by its blob ID with the given input.

        Args:
            flow_id: The blob ID of the flow to evaluate
            input: The input to provide to the flow
            overrides: Optional workflow overrides to apply before execution
            subflow_key: Optional key for subflow deduplication during recovery.
                If not provided, a deterministic key is auto-generated.

        Returns:
            The result value on success

        Raises:
            StepflowFailed: If the flow execution failed with a business logic error
            Exception: For system/runtime errors
        """
        # Use the unified runs API with a single input
        results = await self.evaluate_run_by_id(
            flow_id, [input], overrides=overrides, subflow_key=subflow_key
        )
        # Return the single result
        return results[0]

    async def submit_run(
        self,
        flow: Flow,
        inputs: list[Any],
        wait: bool = False,
        max_concurrency: int | None = None,
        overrides: Any = None,
        subflow_key: uuid.UUID | str | None = None,
    ) -> Any:
        """Submit a run (1 or N items) for execution.

        Args:
            flow: The flow definition to execute
            inputs: List of inputs to process (can be a single-item list)
            wait: If True, wait for completion before returning
            max_concurrency: Maximum number of concurrent executions (optional)
            overrides: Optional workflow overrides to apply
            subflow_key: Optional key for subflow deduplication during recovery.
                If not provided, a deterministic key is auto-generated.

        Returns:
            Run status
        """
        raise NotImplementedError("Subclasses must implement submit_run")

    async def submit_run_by_id(
        self,
        flow_id: str,
        inputs: list[Any],
        wait: bool = False,
        max_concurrency: int | None = None,
        overrides: Any = None,
        subflow_key: uuid.UUID | str | None = None,
    ) -> Any:
        """Submit a run by flow ID.

        Args:
            flow_id: The blob ID of the flow to execute
            inputs: List of inputs to process
            wait: If True, wait for completion before returning
            max_concurrency: Maximum number of concurrent executions (optional)
            overrides: Optional workflow overrides to apply
            subflow_key: Optional key for subflow deduplication during recovery.
                If not provided, a deterministic key is auto-generated.

        Returns:
            Run status
        """
        raise NotImplementedError("Subclasses must implement submit_run_by_id")

    async def get_run(
        self,
        run_id: str,
        wait: bool = False,
        include_results: bool = False,
        **kwargs: Any,
    ) -> Any:
        """Get run status and optionally wait for completion and retrieve results.

        Args:
            run_id: The ID of the run to retrieve
            wait: If True, wait for run completion before returning
            include_results: If True, include the item results in the response

        Returns:
            Run status
        """
        raise NotImplementedError("Subclasses must implement get_run")

    async def evaluate_run(
        self,
        flow: Flow,
        inputs: list[Any],
        max_concurrency: int | None = None,
        overrides: Any = None,
        subflow_key: uuid.UUID | str | None = None,
    ) -> list[Any]:
        """Submit a run, wait for completion, and return all results.

        This is a convenience method that combines submit_run and get_run
        with wait=True and include_results=True.

        Args:
            flow: The flow definition to execute
            inputs: List of inputs to process
            max_concurrency: Maximum number of concurrent executions (optional)
            overrides: Optional workflow overrides to apply
            subflow_key: Optional key for subflow deduplication during recovery.
                If not provided, a deterministic key is auto-generated.

        Returns:
            List of results corresponding to each input, in the same order

        Raises:
            StepflowFailed: If any of the runs failed
        """
        # Convert Flow object to dict for blob storage
        flow_dict = msgspec.to_builtins(flow)

        # Store flow as a blob first
        flow_id = await self.put_blob(flow_dict, BLOB_TYPE_FLOW)

        # Delegate to evaluate_run_by_id
        return await self.evaluate_run_by_id(
            flow_id,
            inputs,
            max_concurrency,
            overrides=overrides,
            subflow_key=subflow_key,
        )

    async def evaluate_run_by_id(
        self,
        flow_id: str,
        inputs: list[Any],
        max_concurrency: int | None = None,
        overrides: Any = None,
        subflow_key: uuid.UUID | str | None = None,
    ) -> list[Any]:
        """Submit a run by flow ID, wait for completion, and return all results.

        Args:
            flow_id: The blob ID of the flow to execute
            inputs: List of inputs to process
            max_concurrency: Maximum number of concurrent executions (optional)
            overrides: Optional workflow overrides to apply
            subflow_key: Optional key for subflow deduplication during recovery.
                If not provided, a deterministic key is auto-generated.

        Returns:
            List of results corresponding to each input, in the same order

        Raises:
            StepflowFailed: If any of the runs failed
        """
        raise NotImplementedError("Subclasses must implement evaluate_run_by_id")
