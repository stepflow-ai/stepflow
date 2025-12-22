#!/bin/bash
# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
# in compliance with the License. You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software distributed under the License
# is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
# or implied. See the License for the specific language governing permissions and limitations under
# the License.

# Generate the stepflow-api client from the OpenAPI spec
#
# This script uses datamodel-code-generator for reliable handling of complex
# OpenAPI schemas including oneOf, allOf, anyOf, and tuple types.
#
# Usage:
#   ./scripts/generate-api-client.sh                    # Regenerate from stored spec
#   ./scripts/generate-api-client.sh --from-spec FILE   # Regenerate from spec file
#   ./scripts/generate-api-client.sh --check            # Check if client is up-to-date
#   ./scripts/generate-api-client.sh --update-spec      # Update the stored spec from server
#
# The stored spec (schemas/openapi.json) should be updated whenever the Rust API changes.
# CI checks that the generated models match this spec.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_SDK_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$(dirname "$PYTHON_SDK_DIR")")"
API_CLIENT_DIR="$PYTHON_SDK_DIR/stepflow-api"
STORED_SPEC="$PROJECT_ROOT/schemas/openapi.json"
TEMP_DIR=$(mktemp -d)
PORT=17837

MODE="generate"
SPEC_FILE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --check)
            MODE="check"
            shift
            ;;
        --update-spec)
            MODE="update-spec"
            shift
            ;;
        --from-spec)
            SPEC_FILE="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

cleanup() {
    if [[ -n "${SERVER_PID:-}" ]]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

fetch_spec_from_server() {
    echo ">>> Building stepflow-server..."
    cd "$PROJECT_ROOT/stepflow-rs"
    cargo build --release -p stepflow-server 2>&1 | tail -3

    echo ">>> Starting stepflow-server on port $PORT..."
    "$PROJECT_ROOT/stepflow-rs/target/release/stepflow-server" --port "$PORT" &
    SERVER_PID=$!

    echo ">>> Waiting for server to be ready..."
    for i in {1..30}; do
        if curl -s "http://localhost:$PORT/health" > /dev/null 2>&1; then
            break
        fi
        if [[ $i -eq 30 ]]; then
            echo "ERROR: Server failed to start"
            exit 1
        fi
        sleep 1
    done

    echo ">>> Fetching OpenAPI spec..."
    curl -s "http://localhost:$PORT/api/v1/openapi.json" > "$1"

    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    unset SERVER_PID
}

generate_models() {
    local spec_file="$1"
    local output_dir="$2"

    echo ">>> Generating Python models with datamodel-code-generator..."

    # Create output structure
    mkdir -p "$output_dir/stepflow_api/models"

    # Generate models using datamodel-code-generator with pydantic v2
    # This handles complex schemas (oneOf, allOf, anyOf, tuple types) correctly
    uvx --from datamodel-code-generator datamodel-codegen \
        --input "$spec_file" \
        --input-file-type openapi \
        --output-model-type pydantic_v2.BaseModel \
        --output "$output_dir/stepflow_api/models/generated.py" \
        --use-annotated \
        --field-constraints \
        --use-standard-collections \
        --use-union-operator \
        --target-python-version 3.11 \
        --enum-field-as-literal one

    # Create stub files for backwards compatibility
    # The existing API code imports from individual model files
    create_model_stubs "$output_dir/stepflow_api/models"

    echo ">>> Models generated successfully"
}

create_model_stubs() {
    local models_dir="$1"

    echo ">>> Creating model stub files for backwards compatibility..."

    # Create Python script to generate stubs
    python3 << EOF
import re
from pathlib import Path

models_dir = Path("$models_dir")
generated_file = models_dir / "generated.py"

# Read the generated file to find class names
content = generated_file.read_text()

# Find all class definitions
classes = re.findall(r'^class (\w+)\(', content, re.MULTILINE)

# Model name mappings (snake_case filename -> CamelCase class name)
# The API code expects certain file names
model_files = {
    "batch_details": "BatchDetails",
    "batch_output_info": "BatchOutputInfo",
    "batch_run_info": "BatchRunInfo",
    "batch_statistics": "BatchStatistics",
    "batch_status": "BatchStatus",
    "cancel_batch_response": "CancelBatchResponse",
    "create_batch_request": "CreateBatchRequest",
    "create_batch_response": "CreateBatchResponse",
    "create_run_request": "CreateRunRequest",
    "create_run_response": "CreateRunResponse",
    "debug_runnable_response": "DebugRunnableResponse",
    "debug_step_request": "DebugStepRequest",
    "debug_step_response": "DebugStepResponse",
    "execution_status": "ExecutionStatus",
    "health_response": "HealthResponse",
    "list_batch_outputs_response": "ListBatchOutputsResponse",
    "list_batch_runs_response": "ListBatchRunsResponse",
    "list_batches_response": "ListBatchesResponse",
    "list_components_response": "ListComponentsResponse",
    "list_runs_response": "ListRunsResponse",
    "list_step_runs_response": "ListStepRunsResponse",
    "run_details": "RunDetails",
    "run_summary": "RunSummary",
    "store_flow_response": "StoreFlowResponse",
    "workflow_overrides": "WorkflowOverrides",
}

# Create stub files
for filename, classname in model_files.items():
    stub_path = models_dir / f"{filename}.py"
    if classname in classes:
        stub_content = f'''"""Re-export {classname} from generated models."""

from .generated import {classname}

__all__ = ["{classname}"]
'''
        stub_path.write_text(stub_content)
        print(f"  Created {filename}.py")
    else:
        print(f"  Skipping {filename}.py - {classname} not found in generated models")

print(f"Created {len([f for f in models_dir.glob('*.py') if f.name not in ['__init__.py', 'generated.py', 'workflow_overrides.py']])} stub files")
EOF
}

generate_models_init() {
    local models_dir="$1"

    cat > "$models_dir/__init__.py" << 'INITEOF'
"""Generated models for the Stepflow API.

This module re-exports all generated models and adds compatibility
methods (to_dict, from_dict) for backwards compatibility.
"""

from __future__ import annotations

import sys
from typing import Any

# Import all from generated
from .generated import *  # noqa: F401, F403

# Collect all exported names for __all__
__all__: list[str] = []

# Get all classes from generated module
from . import generated

for name in dir(generated):
    obj = getattr(generated, name)
    if isinstance(obj, type):
        __all__.append(name)

# Re-export UNSET from types (add to __all__ first to satisfy linter)
__all__.extend(["UNSET", "Unset", "WorkflowOverrides"])
from ..types import UNSET, Unset  # noqa: E402, F401

# Try to import workflow_overrides if custom version exists
try:
    from .workflow_overrides import WorkflowOverrides  # noqa: E402, F401
except ImportError:
    pass


# Add to_dict and from_dict methods to all pydantic models for compatibility
def _add_compatibility_methods():
    """Add to_dict/from_dict methods to all exported models."""
    from pydantic import BaseModel

    module = sys.modules[__name__]

    for name in __all__:
        cls = getattr(module, name, None)
        if cls is None or not isinstance(cls, type):
            continue
        if not issubclass(cls, BaseModel):
            continue

        # Add to_dict method
        if not hasattr(cls, "to_dict"):
            def make_to_dict(c: type) -> Any:
                def to_dict(self: Any) -> dict[str, Any]:
                    return self.model_dump(mode="json", by_alias=True, exclude_unset=True)
                return to_dict
            cls.to_dict = make_to_dict(cls)

        # Add from_dict class method
        if not hasattr(cls, "from_dict"):
            def make_from_dict(c: type) -> Any:
                @classmethod
                def from_dict(cls: type, data: dict[str, Any]) -> Any:
                    return cls.model_validate(data)
                return from_dict
            cls.from_dict = make_from_dict(cls)


_add_compatibility_methods()
INITEOF
}

generate_client_base() {
    local output_dir="$1"

    echo ">>> Generating client base files..."

    # Create the client module (compatible with existing API code)
    cat > "$output_dir/stepflow_api/client.py" << 'CLIENTEOF'
"""HTTP client for Stepflow API."""

from __future__ import annotations

import httpx


class Client:
    """HTTP client wrapper for Stepflow API."""

    def __init__(
        self,
        base_url: str,
        *,
        timeout: httpx.Timeout | None = None,
        headers: dict[str, str] | None = None,
        raise_on_unexpected_status: bool = True,
    ):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout or httpx.Timeout(300.0)
        self.headers = headers or {}
        self.raise_on_unexpected_status = raise_on_unexpected_status
        self._async_client: httpx.AsyncClient | None = None
        self._sync_client: httpx.Client | None = None

    def get_httpx_client(self) -> httpx.Client:
        """Get synchronous httpx client."""
        if self._sync_client is None:
            self._sync_client = httpx.Client(
                base_url=self.base_url,
                timeout=self.timeout,
                headers=self.headers,
            )
        return self._sync_client

    def get_async_httpx_client(self) -> httpx.AsyncClient:
        """Get asynchronous httpx client."""
        if self._async_client is None:
            self._async_client = httpx.AsyncClient(
                base_url=self.base_url,
                timeout=self.timeout,
                headers=self.headers,
            )
        return self._async_client

    async def aclose(self) -> None:
        """Close async client."""
        if self._async_client is not None:
            await self._async_client.aclose()
            self._async_client = None

    def close(self) -> None:
        """Close sync client."""
        if self._sync_client is not None:
            self._sync_client.close()
            self._sync_client = None


# Alias for backwards compatibility
AuthenticatedClient = Client
CLIENTEOF

    # Create types module
    cat > "$output_dir/stepflow_api/types.py" << 'TYPESEOF'
"""Common types for the API client."""

from __future__ import annotations

from dataclasses import dataclass
from http import HTTPStatus
from typing import Any, Generic, TypeVar


# Sentinel for unset values (compatible with pydantic's exclude_unset)
class _Unset:
    def __bool__(self) -> bool:
        return False

    def __repr__(self) -> str:
        return "UNSET"


UNSET = _Unset()
UnsetType = _Unset
Unset = _Unset  # Alias for compatibility


T = TypeVar("T")


@dataclass
class Response(Generic[T]):
    """HTTP response wrapper."""

    status_code: HTTPStatus
    content: bytes
    headers: dict[str, Any]
    parsed: T | None = None
TYPESEOF

    # Create errors module
    cat > "$output_dir/stepflow_api/errors.py" << 'ERRORSEOF'
"""Error types for the API client."""


class UnexpectedStatus(Exception):
    """Raised when the server returns an unexpected status code."""

    def __init__(self, status_code: int, content: bytes):
        self.status_code = status_code
        self.content = content
        super().__init__(f"Unexpected status code: {status_code}")
ERRORSEOF

    # Create package __init__.py
    cat > "$output_dir/stepflow_api/__init__.py" << 'INITEOF'
"""Stepflow API client."""

from .client import AuthenticatedClient, Client
from .errors import UnexpectedStatus
from .types import UNSET, Response, Unset, UnsetType

__all__ = [
    "AuthenticatedClient",
    "Client",
    "Response",
    "UNSET",
    "Unset",
    "UnexpectedStatus",
    "UnsetType",
]
INITEOF

    # Create py.typed marker
    touch "$output_dir/stepflow_api/py.typed"
}

compare_models() {
    local generated_file="$1"
    local existing_file="$2"

    # Compare the generated models file
    if [[ ! -f "$existing_file" ]]; then
        echo "ERROR: No existing models file at $existing_file"
        return 1
    fi

    # Compare files ignoring the timestamp line (line 3 contains a timestamp that changes every run)
    # datamodel-code-generator adds: "#   timestamp: 2025-12-21T19:54:41+00:00"
    if ! diff <(sed '3d' "$generated_file") <(sed '3d' "$existing_file") > /dev/null 2>&1; then
        return 1
    fi

    return 0
}

echo "=== stepflow-api client management ==="

case "$MODE" in
    "update-spec")
        echo ">>> Updating stored OpenAPI spec..."
        fetch_spec_from_server "$STORED_SPEC"
        echo ">>> Stored spec updated: $STORED_SPEC"
        echo ""
        echo "Now regenerate the client with: ./scripts/generate-api-client.sh"
        ;;

    "check")
        echo ">>> Checking if models match stored spec..."

        if [[ ! -f "$STORED_SPEC" ]]; then
            echo "ERROR: No stored spec at $STORED_SPEC"
            echo "Run: ./scripts/generate-api-client.sh --update-spec"
            exit 1
        fi

        generate_models "$STORED_SPEC" "$TEMP_DIR/generated"
        generate_models_init "$TEMP_DIR/generated/stepflow_api/models"

        # Format the generated files to match the project style
        # Must format in the correct location so ruff uses proper path-based settings
        TEMP_MODELS_DIR="$API_CLIENT_DIR/src/stepflow_api/models-check-temp"
        rm -rf "$TEMP_MODELS_DIR"
        cp -r "$TEMP_DIR/generated/stepflow_api/models" "$TEMP_MODELS_DIR"
        cd "$PYTHON_SDK_DIR"
        if command -v uv &> /dev/null; then
            uv run ruff format "$TEMP_MODELS_DIR/" 2>/dev/null || true
        fi
        # Copy formatted files back
        rm -rf "$TEMP_DIR/generated/stepflow_api/models"
        mv "$TEMP_MODELS_DIR" "$TEMP_DIR/generated/stepflow_api/models"

        EXISTING_MODELS="$API_CLIENT_DIR/src/stepflow_api/models/generated.py"
        if [[ ! -f "$EXISTING_MODELS" ]]; then
            echo "ERROR: No existing models at $EXISTING_MODELS"
            echo "Run: ./scripts/generate-api-client.sh"
            exit 1
        fi

        if ! compare_models "$TEMP_DIR/generated/stepflow_api/models/generated.py" "$EXISTING_MODELS"; then
            echo "ERROR: stepflow-api models are out of date!"
            echo ""
            echo "Differences detected in generated models."
            echo ""
            echo "To fix:"
            echo "  1. If the Rust API changed: ./scripts/generate-api-client.sh --update-spec"
            echo "  2. Regenerate models: ./scripts/generate-api-client.sh"
            exit 1
        fi

        echo ">>> stepflow-api models are up-to-date"
        ;;

    "generate")
        # Determine spec source
        if [[ -n "$SPEC_FILE" ]]; then
            OPENAPI_SPEC="$SPEC_FILE"
        elif [[ -f "$STORED_SPEC" ]]; then
            OPENAPI_SPEC="$STORED_SPEC"
            echo ">>> Using stored spec: $STORED_SPEC"
        else
            echo ">>> No stored spec found, fetching from server..."
            OPENAPI_SPEC="$TEMP_DIR/openapi.json"
            fetch_spec_from_server "$OPENAPI_SPEC"
        fi

        # Generate to temp directory first
        generate_models "$OPENAPI_SPEC" "$TEMP_DIR/generated"
        generate_models_init "$TEMP_DIR/generated/stepflow_api/models"
        generate_client_base "$TEMP_DIR/generated"

        # Preserve existing API endpoint modules (they rarely change)
        if [[ -d "$API_CLIENT_DIR/src/stepflow_api/api" ]]; then
            echo ">>> Preserving existing API endpoint modules..."
            cp -r "$API_CLIENT_DIR/src/stepflow_api/api" "$TEMP_DIR/generated/stepflow_api/"
        fi

        # Preserve custom workflow_overrides.py if it exists
        if [[ -f "$API_CLIENT_DIR/src/stepflow_api/models/workflow_overrides.py" ]]; then
            cp "$API_CLIENT_DIR/src/stepflow_api/models/workflow_overrides.py" \
               "$TEMP_DIR/generated/stepflow_api/models/"
        fi

        # Replace the package
        rm -rf "$API_CLIENT_DIR/src/stepflow_api"
        mkdir -p "$API_CLIENT_DIR/src"
        cp -r "$TEMP_DIR/generated/stepflow_api" "$API_CLIENT_DIR/src/"

        # Format generated files to match project style
        echo ">>> Formatting generated files..."
        cd "$PYTHON_SDK_DIR"
        if command -v uv &> /dev/null; then
            uv run ruff format "$API_CLIENT_DIR/src/stepflow_api/models/" 2>/dev/null || true
        fi

        echo ">>> Client updated: $API_CLIENT_DIR/src/stepflow_api"
        echo ""
        echo "NOTE: Models regenerated with pydantic v2. API endpoint modules preserved."
        echo "      Models include to_dict()/from_dict() compatibility methods."
        echo "      Individual model stub files created for backwards compatibility."
        ;;
esac

echo "=== Done ==="
