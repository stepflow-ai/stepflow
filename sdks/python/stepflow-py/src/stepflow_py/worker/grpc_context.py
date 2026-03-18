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

"""gRPC-based StepflowContext for pull-based workers.

Uses gRPC clients for OrchestratorService (submit_run, get_run) and
BlobService (put_blob, get_blob).

The orchestrator_service_url comes from TaskContext in each
TaskAssignment (varies per task in multi-orchestrator deployments).
The blob URL and threshold come from environment variables set at
deployment time (STEPFLOW_BLOB_URL, STEPFLOW_BLOB_THRESHOLD_BYTES).
"""

from __future__ import annotations

import logging
from typing import Any

import grpc.aio
from google.protobuf import struct_pb2
from google.protobuf.json_format import MessageToDict

from stepflow_py.proto import (
    CreateRunRequest,
    GetBlobRequest,
    OrchestratorGetRunRequest,
    OrchestratorSubmitRunRequest,
    PutBlobRequest,
)
from stepflow_py.proto.blobs_pb2_grpc import BlobServiceStub
from stepflow_py.proto.orchestrator_pb2_grpc import OrchestratorServiceStub
from stepflow_py.worker.context import StepflowContext

logger = logging.getLogger(__name__)


class GrpcContext(StepflowContext):
    """StepflowContext implementation for gRPC pull-based workers.

    Uses gRPC for OrchestratorService (submit_run, get_run) and
    BlobService (put_blob, get_blob). All URLs come from
    the TaskContext in the TaskAssignment or environment variables.
    """

    def __init__(
        self,
        orchestrator_url: str,
        blob_url: str,
        blob_threshold: int = 0,
        run_id: str | None = None,
        flow_id: str | None = None,
        step_id: str | None = None,
        attempt: int = 1,
    ):
        # Initialize the parent with minimal params — we override the
        # methods that actually talk to the runtime.
        # We pass None for queue/decoder/http_client since we use gRPC instead.
        super().__init__(
            outgoing_queue=None,  # type: ignore[arg-type]
            message_decoder=None,  # type: ignore[arg-type]
            http_client=None,
            run_id=run_id,
            flow_id=flow_id,
            step_id=step_id,
            attempt=attempt,
            blob_api_url=blob_url if blob_url else None,
        )
        self._orchestrator_url = orchestrator_url
        self._blob_url = blob_url
        self._blob_threshold = blob_threshold

    async def put_blob(self, data: Any, blob_type: str = "data") -> str:
        """Store data as a blob via the BlobService gRPC API."""
        if not self._blob_url:
            raise RuntimeError("No blob_service_url configured")

        if isinstance(data, bytes | bytearray):
            request = PutBlobRequest(
                raw_data=bytes(data),
                blob_type=blob_type,
            )
        else:
            json_data = _python_to_proto_value(data)
            request = PutBlobRequest(
                json_data=json_data,
                blob_type=blob_type,
            )

        channel = grpc.aio.insecure_channel(self._blob_url)
        try:
            stub = BlobServiceStub(channel)
            response = await stub.PutBlob(request)
            return response.blob_id
        finally:
            await channel.close()

    async def get_blob(self, blob_id: str) -> Any:
        """Retrieve blob data via the BlobService gRPC API."""
        if not self._blob_url:
            raise RuntimeError("No blob_service_url configured")

        request = GetBlobRequest(blob_id=blob_id)

        channel = grpc.aio.insecure_channel(self._blob_url)
        try:
            stub = BlobServiceStub(channel)
            response = await stub.GetBlob(request)
            # Extract JSON data from the oneof content field
            if response.HasField("json_data"):
                return _proto_value_to_python(response.json_data)
            elif response.HasField("raw_data"):
                return response.raw_data
            else:
                return None
        finally:
            await channel.close()

    async def submit_run(  # type: ignore[override]
        self,
        flow: Any | None = None,
        inputs: list[Any] | None = None,
        *,
        flow_id: str | None = None,
        wait: bool = True,
        timeout_secs: int | None = None,
        max_concurrency: int | None = None,
        overrides: dict[str, Any] | None = None,
    ) -> Any:
        """Submit a run via OrchestratorService.SubmitRun."""
        if not self._orchestrator_url:
            raise RuntimeError("No orchestrator_service_url configured")

        # If a flow object is provided, store it as a blob first
        if flow is not None and flow_id is None:
            flow_id = await self.put_blob(flow, blob_type="flow")

        if flow_id is None:
            raise ValueError("Either flow or flow_id must be provided")

        # Convert inputs to proto Values
        proto_inputs = []
        for inp in inputs or [{}]:
            v = _python_to_proto_value(inp)
            proto_inputs.append(v)

        # Convert overrides to proto Struct
        proto_overrides = None
        if overrides:
            proto_overrides = struct_pb2.Struct()
            for k, v in overrides.items():
                proto_overrides.fields[str(k)].CopyFrom(_python_to_proto_value(v))

        # Build the shared CreateRunRequest
        run_request = CreateRunRequest(
            flow_id=flow_id,
            input=proto_inputs,
            wait=wait,
        )
        if max_concurrency is not None:
            run_request.max_concurrency = max_concurrency
        if timeout_secs is not None:
            run_request.timeout_secs = timeout_secs
        if proto_overrides is not None:
            run_request.overrides.CopyFrom(proto_overrides)

        # Wrap in OrchestratorSubmitRunRequest
        request = OrchestratorSubmitRunRequest(
            run_request=run_request,
        )

        channel = grpc.aio.insecure_channel(self._orchestrator_url)
        try:
            stub = OrchestratorServiceStub(channel)
            response = await stub.SubmitRun(request)
            return MessageToDict(response, preserving_proto_field_name=True)
        finally:
            await channel.close()

    async def get_run(  # type: ignore[override]
        self,
        run_id: str,
        *,
        wait: bool = False,
        include_results: bool = True,
        timeout_secs: int | None = None,
    ) -> Any:
        """Get run status via OrchestratorService.GetRun."""
        if not self._orchestrator_url:
            raise RuntimeError("No orchestrator_service_url configured")

        request = OrchestratorGetRunRequest(
            run_id=run_id,
            wait=wait,
            include_results=include_results,
        )
        if timeout_secs is not None:
            request.timeout_secs = timeout_secs

        channel = grpc.aio.insecure_channel(self._orchestrator_url)
        try:
            stub = OrchestratorServiceStub(channel)
            response = await stub.GetRun(request)
            return MessageToDict(response, preserving_proto_field_name=True)
        finally:
            await channel.close()

    async def submit_run_by_id(  # type: ignore[override]
        self,
        flow_id: str,
        inputs: list[Any],
        wait: bool = False,
        max_concurrency: int | None = None,
        overrides: Any = None,
        subflow_key: Any = None,
    ) -> Any:
        """Submit a run by flow ID via OrchestratorService.SubmitRun."""
        return await self.submit_run(
            flow_id=flow_id,
            inputs=inputs,
            wait=wait,
            max_concurrency=max_concurrency,
            overrides=overrides,
        )

    async def evaluate_run_by_id(  # type: ignore[override]
        self,
        flow_id: str,
        inputs: list[Any],
        max_concurrency: int | None = None,
        overrides: Any = None,
        subflow_key: Any = None,
    ) -> list[Any]:
        """Submit a run by flow ID, wait for completion, and return results.

        Uses gRPC SubmitRun with wait=True, then extracts results from
        the OrchestratorRunStatus dict response.
        """
        from stepflow_py.worker.exceptions import StepflowFailed

        run_status = await self.submit_run(
            flow_id=flow_id,
            inputs=inputs,
            wait=True,
            max_concurrency=max_concurrency,
            overrides=overrides,
        )

        item_results = run_status.get("results", [])
        if not item_results:
            raise Exception("Expected results in response when wait=True")

        results = []
        for item in item_results:
            status = item.get("status", "")
            if status == "EXECUTION_STATUS_FAILED":
                error_msg = item.get("error_message", "Unknown error")
                error_code = item.get("error_code", "TASK_ERROR_CODE_UNSPECIFIED")
                raise StepflowFailed(
                    error_code=error_code,
                    message=f"Item at index {item.get('item_index', '?')} failed: {error_msg}",
                )
            output = item.get("output")
            if output is None:
                raise Exception(
                    f"Item at index {item.get('item_index', '?')} has no output "
                    f"(status: {status})"
                )
            results.append(output)

        return results

    @property
    def attempt(self) -> int:
        """Current execution attempt number."""
        return self._attempt


def _python_to_proto_value(obj: Any) -> struct_pb2.Value:
    """Convert a Python object to a protobuf Value.

    Delegates to grpc_worker's implementation which handles non-string dict keys.
    """
    from stepflow_py.worker.grpc_worker import (
        _python_to_proto_value as _impl,
    )

    return _impl(obj)


def _proto_value_to_python(value: struct_pb2.Value) -> Any:
    """Convert a protobuf Value to a Python object.

    Delegates to grpc_worker's implementation which preserves integer types.
    """
    from stepflow_py.worker.grpc_worker import (
        _proto_value_to_python as _impl,
    )

    return _impl(value)
