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
from google.protobuf.json_format import MessageToDict, ParseDict

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
            ParseDict(overrides, proto_overrides)

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

    @property
    def attempt(self) -> int:
        """Current execution attempt number."""
        return self._attempt


def _python_to_proto_value(obj: Any) -> struct_pb2.Value:
    """Convert a Python object to a protobuf Value."""
    value = struct_pb2.Value()
    if obj is None:
        value.null_value = struct_pb2.NULL_VALUE
    elif isinstance(obj, bool):
        value.bool_value = obj
    elif isinstance(obj, int | float):
        value.number_value = float(obj)
    elif isinstance(obj, str):
        value.string_value = obj
    elif isinstance(obj, dict):
        struct = struct_pb2.Struct()
        ParseDict(obj, struct)
        value.struct_value.CopyFrom(struct)
    elif isinstance(obj, list):
        list_value = struct_pb2.ListValue()
        for item in obj:
            list_value.values.append(_python_to_proto_value(item))
        value.list_value.CopyFrom(list_value)
    else:
        import json

        try:
            d = json.loads(json.dumps(obj, default=str))
            return _python_to_proto_value(d)
        except Exception:
            value.string_value = str(obj)
    return value


def _proto_value_to_python(value: struct_pb2.Value) -> Any:
    """Convert a protobuf Value to a Python object."""
    return MessageToDict(value, preserving_proto_field_name=True)
