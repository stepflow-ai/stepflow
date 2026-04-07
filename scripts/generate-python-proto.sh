#!/usr/bin/env bash
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

# Generate Python gRPC stubs from proto files.
#
# Generates stubs for protos needed by the Python worker and API client:
# common, tasks, orchestrator, blobs, runs, flows, health, components.
#
# Note: Some protos use google.api.http annotations for REST transcoding,
# but grpcio-tools handles them fine — it just ignores the HTTP annotations.
#
# Usage:
#   ./scripts/generate-python-proto.sh
#
# Prerequisites (in dev dependencies):
#   grpcio-tools mypy-protobuf

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROTO_DIR="$ROOT_DIR/stepflow-rs/crates/stepflow-proto/proto"
OUT_DIR="$ROOT_DIR/sdks/python/stepflow-py/src/stepflow_py/proto"

echo "Generating Python gRPC stubs..."
echo "  Proto dir: $PROTO_DIR"
echo "  Output dir: $OUT_DIR"

# Clean and recreate output directory
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

# Generate Python + gRPC stubs with mypy type stubs for IDE support.
# Include google/api and gnostic annotation protos so the generated
# blobs_pb2.py and runs_pb2.py can import them at runtime.
cd "$ROOT_DIR/sdks/python"
uv run --project stepflow-py python -m grpc_tools.protoc \
    --proto_path="$PROTO_DIR" \
    --python_out="$OUT_DIR" \
    --grpc_python_out="$OUT_DIR" \
    --mypy_out="$OUT_DIR" \
    --mypy_grpc_out="$OUT_DIR" \
    stepflow/v1/common.proto \
    stepflow/v1/tasks.proto \
    stepflow/v1/orchestrator.proto \
    stepflow/v1/blobs.proto \
    stepflow/v1/runs.proto \
    stepflow/v1/flows.proto \
    stepflow/v1/health.proto \
    stepflow/v1/components.proto \
    stepflow/v1/vsock.proto \
    google/api/annotations.proto \
    google/api/http.proto \
    gnostic/openapi/v3/annotations.proto \
    gnostic/openapi/v3/openapiv3.proto

# protoc outputs files in a nested stepflow/v1/ directory matching the package.
# Move them to the flat output directory for simpler imports.
mv "$OUT_DIR"/stepflow/v1/* "$OUT_DIR"/
rm -rf "$OUT_DIR/stepflow"

# Keep generated stubs for google.api and gnostic annotation protos —
# blobs_pb2.py and runs_pb2.py import them at module load time.
# Move them into the proto package so they're importable.
if [ -d "$OUT_DIR/google" ]; then
    # Create __init__.py files for the google.api and gnostic packages
    # so they're importable as sub-packages of the proto module.
    touch "$OUT_DIR/google/__init__.py"
    touch "$OUT_DIR/google/api/__init__.py"
fi
if [ -d "$OUT_DIR/gnostic" ]; then
    touch "$OUT_DIR/gnostic/__init__.py"
    touch "$OUT_DIR/gnostic/openapi/__init__.py"
    touch "$OUT_DIR/gnostic/openapi/v3/__init__.py"
fi

# Fix imports in generated files: protoc generates absolute imports like
# "from stepflow.v1 import common_pb2" but we need relative imports since
# the files are now flat in the proto package.
echo "Fixing imports in generated files..."
# Use perl for portable in-place editing (works on both macOS and Linux,
# unlike sed -i which differs between BSD and GNU).
find "$OUT_DIR" -name "*.py" -exec perl -pi -e \
    's/from stepflow\.v1 import/from . import/g' \
    {} +

find "$OUT_DIR" -name "*.pyi" -exec perl -pi -e \
    's/from stepflow\.v1 import/from . import/g' \
    {} +

# Fix .pyi bare imports and qualified references: mypy-protobuf generates
# "import stepflow.v1.common_pb2" and "stepflow.v1.common_pb2.SomeType".
# Rewrite to "from . import common_pb2" and "common_pb2.SomeType".
find "$OUT_DIR" -maxdepth 1 -name "*.pyi" -exec perl -pi -e \
    's/^import stepflow\.v1\.(\w+)/from . import $1/g' \
    {} +

find "$OUT_DIR" -maxdepth 1 -name "*.pyi" -exec perl -pi -e \
    's/stepflow\.v1\.//g' \
    {} +

# Fix gnostic imports: generated files import "from gnostic.openapi.v3 import ..."
# which isn't an installable Python package. Rewrite to relative imports.
# Top-level files (blobs_pb2.py, runs_pb2.py) → ".gnostic.openapi.v3"
find "$OUT_DIR" -maxdepth 1 -name "*.py" -exec perl -pi -e \
    's/from gnostic\.openapi\.v3 import/from .gnostic.openapi.v3 import/g' \
    {} +

find "$OUT_DIR" -maxdepth 1 -name "*.pyi" -exec perl -pi -e \
    's/from gnostic\.openapi\.v3 import/from .gnostic.openapi.v3 import/g' \
    {} +

# Fix gnostic bare imports in .pyi files
find "$OUT_DIR" -maxdepth 1 -name "*.pyi" -exec perl -pi -e \
    's/^import gnostic\.openapi\.v3\.(\w+)/from .gnostic.openapi.v3 import $1/g' \
    {} +

find "$OUT_DIR" -maxdepth 1 -name "*.pyi" -exec perl -pi -e \
    's/gnostic\.openapi\.v3\.//g' \
    {} +

# Files within gnostic/openapi/v3/ → relative "from . import"
find "$OUT_DIR/gnostic" -name "*.py" -exec perl -pi -e \
    's/from gnostic\.openapi\.v3 import/from . import/g' \
    {} +

find "$OUT_DIR/gnostic" -name "*.pyi" -exec perl -pi -e \
    's/from gnostic\.openapi\.v3 import/from . import/g' \
    {} +

# Fix gnostic bare imports/refs in nested .pyi files
find "$OUT_DIR/gnostic" -name "*.pyi" -exec perl -pi -e \
    's/^import gnostic\.openapi\.v3\.(\w+)/from . import $1/g' \
    {} +

find "$OUT_DIR/gnostic" -name "*.pyi" -exec perl -pi -e \
    's/gnostic\.openapi\.v3\.//g' \
    {} +

# Create __init__.py with convenience re-exports
cat > "$OUT_DIR/__init__.py" << 'EOF'
"""Generated protobuf and gRPC stubs for Stepflow v1 protocol.

Do not edit — regenerate with scripts/generate-python-proto.sh
"""

from .common_pb2 import (
    ObservabilityContext,
)
from .tasks_pb2 import (
    ComponentExecuteRequest,
    ComponentExecuteResponse,
    ComponentInfo,
    ComponentInfoRequest,
    ComponentInfoResponse,
    GetOrchestratorForRunRequest,
    GetOrchestratorForRunResponse,
    ListComponentsRequest,
    ListComponentsResponse,
    PullTasksRequest,
    TaskAssignment,
    TaskContext,
)
from .tasks_pb2_grpc import TasksServiceStub
from .orchestrator_pb2 import (
    TaskError,
    TaskStatus,
    CompleteTaskRequest,
    CompleteTaskResponse,
    ListComponentsResult,
    OrchestratorGetRunRequest,
    OrchestratorRunStatus,
    OrchestratorSubmitRunRequest,
    TaskHeartbeatRequest,
    TaskHeartbeatResponse,
)
from .blobs_pb2 import (
    GetBlobRequest,
    GetBlobResponse,
    PutBlobRequest,
    PutBlobResponse,
)
from .runs_pb2 import (
    CreateRunRequest,
    CreateRunResponse,
    GetRunRequest,
    GetRunResponse,
    GetRunEventsRequest,
    GetRunItemsRequest,
    GetRunItemsResponse,
    ListRunsRequest,
    ListRunsResponse,
    RunSummary,
    StatusEvent,
    StatusEventType,
)
from .flows_pb2 import (
    StoreFlowRequest as StoreFlowRequestProto,
    StoreFlowResponse as StoreFlowResponseProto,
    GetFlowRequest,
    GetFlowResponse,
    GetFlowVariablesRequest,
    GetFlowVariablesResponse,
)
from .health_pb2 import (
    HealthCheckRequest,
    HealthCheckResponse,
)
from .components_pb2 import (
    ListRegisteredComponentsRequest,
    ListRegisteredComponentsResponse,
)
from .vsock_pb2 import (
    VsockTaskEnvelope,
)

# gRPC service stubs
from .runs_pb2_grpc import RunsServiceStub
from .flows_pb2_grpc import FlowsServiceStub
from .health_pb2_grpc import HealthServiceStub
from .blobs_pb2_grpc import BlobServiceStub
from .components_pb2_grpc import ComponentsServiceStub

__all__ = [
    "ObservabilityContext",
    "TaskContext",
    "TaskError",
    "ComponentExecuteRequest",
    "ComponentExecuteResponse",
    "ComponentInfo",
    "ComponentInfoRequest",
    "ComponentInfoResponse",
    "ListComponentsRequest",
    "ListComponentsResponse",
    "PullTasksRequest",
    "TaskAssignment",
    "GetOrchestratorForRunRequest",
    "GetOrchestratorForRunResponse",
    "TasksServiceStub",
    "CompleteTaskRequest",
    "CompleteTaskResponse",
    "ListComponentsResult",
    "OrchestratorGetRunRequest",
    "OrchestratorRunStatus",
    "OrchestratorSubmitRunRequest",
    "TaskStatus",
    "TaskHeartbeatRequest",
    "TaskHeartbeatResponse",
    "GetBlobRequest",
    "GetBlobResponse",
    "PutBlobRequest",
    "PutBlobResponse",
    "CreateRunRequest",
    "CreateRunResponse",
    "GetRunRequest",
    "GetRunResponse",
    "GetRunEventsRequest",
    "GetRunItemsRequest",
    "GetRunItemsResponse",
    "ListRunsRequest",
    "ListRunsResponse",
    "RunSummary",
    "StatusEvent",
    "StatusEventType",
    "StoreFlowRequestProto",
    "StoreFlowResponseProto",
    "GetFlowRequest",
    "GetFlowResponse",
    "GetFlowVariablesRequest",
    "GetFlowVariablesResponse",
    "HealthCheckRequest",
    "HealthCheckResponse",
    "ListRegisteredComponentsRequest",
    "ListRegisteredComponentsResponse",
    "RunsServiceStub",
    "FlowsServiceStub",
    "HealthServiceStub",
    "BlobServiceStub",
    "ComponentsServiceStub",
    "VsockTaskEnvelope",
]
EOF

echo "Done. Generated stubs in $OUT_DIR"
