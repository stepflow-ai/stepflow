#!/usr/bin/env python3
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

"""Generate the stepflow-api client from the OpenAPI spec.

This script uses datamodel-code-generator for reliable handling of complex
OpenAPI schemas including oneOf, allOf, anyOf, and tuple types.

Usage:
    python scripts/generate_api_client.py                    # Regenerate from stored spec
    python scripts/generate_api_client.py --from-spec FILE   # Regenerate from spec file
    python scripts/generate_api_client.py --check            # Check if client is up-to-date
    python scripts/generate_api_client.py --update-spec      # Update stored spec from server

The stored spec (schemas/openapi.json) should be updated whenever the Rust API changes.
CI checks that the generated models match this spec.
"""

from __future__ import annotations

import argparse
import atexit
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
PYTHON_SDK_DIR = SCRIPT_DIR.parent
PROJECT_ROOT = PYTHON_SDK_DIR.parent.parent
API_CLIENT_DIR = PYTHON_SDK_DIR / "stepflow-api"
STORED_SPEC = PROJECT_ROOT / "schemas" / "openapi.json"
PORT = 17837

# Global for cleanup
_server_process: subprocess.Popen | None = None
_temp_dir: Path | None = None


def cleanup():
    """Clean up server process and temp directory."""
    global _server_process, _temp_dir
    if _server_process is not None:
        _server_process.terminate()
        try:
            _server_process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            _server_process.kill()
        _server_process = None
    if _temp_dir is not None and _temp_dir.exists():
        shutil.rmtree(_temp_dir)
        _temp_dir = None


atexit.register(cleanup)
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
signal.signal(signal.SIGINT, lambda *_: sys.exit(0))


def fetch_spec_from_server(output_path: Path) -> None:
    """Build and start server, fetch OpenAPI spec."""
    global _server_process

    print(">>> Building stepflow-server...")
    subprocess.run(
        ["cargo", "build", "--release", "-p", "stepflow-server"],
        cwd=PROJECT_ROOT / "stepflow-rs",
        check=True,
        capture_output=True,
    )

    print(f">>> Starting stepflow-server on port {PORT}...")
    server_bin = PROJECT_ROOT / "stepflow-rs" / "target" / "release" / "stepflow-server"
    _server_process = subprocess.Popen(
        [str(server_bin), "--port", str(PORT)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    print(">>> Waiting for server to be ready...")
    for i in range(30):
        try:
            urllib.request.urlopen(f"http://localhost:{PORT}/health", timeout=1)
            break
        except Exception:
            if i == 29:
                raise RuntimeError("Server failed to start")
            time.sleep(1)

    print(">>> Fetching OpenAPI spec...")
    with urllib.request.urlopen(f"http://localhost:{PORT}/api/v1/openapi.json") as resp:
        output_path.write_bytes(resp.read())

    _server_process.terminate()
    _server_process.wait()
    _server_process = None


def generate_models(spec_file: Path, output_dir: Path) -> None:
    """Generate Python models using datamodel-code-generator."""
    print(">>> Generating Python models with datamodel-code-generator...")

    models_dir = output_dir / "stepflow_api" / "models"
    models_dir.mkdir(parents=True, exist_ok=True)

    subprocess.run(
        [
            "uvx", "--from", "datamodel-code-generator", "datamodel-codegen",
            "--input", str(spec_file),
            "--input-file-type", "openapi",
            "--output-model-type", "pydantic_v2.BaseModel",
            "--output", str(models_dir / "generated.py"),
            "--use-annotated",
            "--field-constraints",
            "--use-standard-collections",
            "--use-union-operator",
            "--target-python-version", "3.11",
            "--enum-field-as-literal", "one",
        ],
        check=True,
    )

    create_model_stubs(models_dir)
    print(">>> Models generated successfully")


def create_model_stubs(models_dir: Path) -> None:
    """Create stub files for backwards compatibility."""
    print(">>> Creating model stub files for backwards compatibility...")

    generated_file = models_dir / "generated.py"
    content = generated_file.read_text()

    # Find all class definitions
    classes = set(re.findall(r"^class (\w+)\(", content, re.MULTILINE))

    # Model name mappings (snake_case filename -> CamelCase class name)
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

    for filename, classname in model_files.items():
        stub_path = models_dir / f"{filename}.py"
        if classname in classes:
            stub_path.write_text(f'''"""Re-export {classname} from generated models.

DO NOT EDIT - Generated by scripts/generate_api_client.py
To customize this model, replace this file with a custom implementation.
Custom files (not starting with this docstring pattern) are preserved during regeneration.
"""

from .generated import {classname}

__all__ = ["{classname}"]
''')
            print(f"  Created {filename}.py")


def generate_models_init(models_dir: Path, custom_overrides: list[str] | None = None) -> None:
    """Generate __init__.py for models package."""
    custom_overrides = custom_overrides or []

    # Build custom override imports
    custom_imports = ""
    if custom_overrides:
        custom_imports = "\n# Import custom model overrides (replace generated versions)\n"
        for name in custom_overrides:
            # Convert snake_case filename to CamelCase class name
            class_name = "".join(word.capitalize() for word in name.replace(".py", "").split("_"))
            custom_imports += f"from .{name.replace('.py', '')} import {class_name}  # noqa: E402, F401\n"

    (models_dir / "__init__.py").write_text(f'''\
"""Generated models for the Stepflow API.

DO NOT EDIT - Generated by scripts/generate_api_client.py

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
__all__.extend(["UNSET", "Unset"])
from ..types import UNSET, Unset  # noqa: E402, F401
{custom_imports}

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
''')


def generate_client_base(output_dir: Path) -> None:
    """Generate client base files."""
    print(">>> Generating client base files...")

    pkg_dir = output_dir / "stepflow_api"
    pkg_dir.mkdir(parents=True, exist_ok=True)

    # client.py
    (pkg_dir / "client.py").write_text('''\
"""HTTP client for Stepflow API.

DO NOT EDIT - Generated by scripts/generate_api_client.py
"""

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
''')

    # types.py
    (pkg_dir / "types.py").write_text('''\
"""Common types for the API client.

DO NOT EDIT - Generated by scripts/generate_api_client.py
"""

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
''')

    # errors.py
    (pkg_dir / "errors.py").write_text('''\
"""Error types for the API client.

DO NOT EDIT - Generated by scripts/generate_api_client.py
"""


class UnexpectedStatus(Exception):
    """Raised when the server returns an unexpected status code."""

    def __init__(self, status_code: int, content: bytes):
        self.status_code = status_code
        self.content = content
        super().__init__(f"Unexpected status code: {status_code}")
''')

    # __init__.py
    (pkg_dir / "__init__.py").write_text('''\
"""Stepflow API client.

DO NOT EDIT - Generated by scripts/generate_api_client.py
"""

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
''')

    # py.typed marker
    (pkg_dir / "py.typed").touch()


def compare_models(generated_file: Path, existing_file: Path) -> bool:
    """Compare generated models, ignoring timestamp line."""
    if not existing_file.exists():
        return False

    gen_lines = generated_file.read_text().splitlines()
    exist_lines = existing_file.read_text().splitlines()

    # Remove timestamp line (line 3) which changes every run
    if len(gen_lines) > 2:
        gen_lines.pop(2)
    if len(exist_lines) > 2:
        exist_lines.pop(2)

    return gen_lines == exist_lines


def format_generated_files(directory: Path) -> None:
    """Format generated files with ruff using project config."""
    subprocess.run(
        ["uvx", "ruff@0.9.4", "format", "--config", str(PYTHON_SDK_DIR / "pyproject.toml"), str(directory)],
        capture_output=True,
    )


def main():
    parser = argparse.ArgumentParser(description="Generate stepflow-api client")
    parser.add_argument("--check", action="store_true", help="Check if client is up-to-date")
    parser.add_argument("--update-spec", action="store_true", help="Update stored spec from server")
    parser.add_argument("--from-spec", type=Path, help="Generate from specific spec file")
    args = parser.parse_args()

    global _temp_dir
    _temp_dir = Path(tempfile.mkdtemp())

    print("=== stepflow-api client management ===")

    if args.update_spec:
        print(">>> Updating stored OpenAPI spec...")
        fetch_spec_from_server(STORED_SPEC)
        print(f">>> Stored spec updated: {STORED_SPEC}")
        print()
        print("Now regenerate the client with: python scripts/generate_api_client.py")

    elif args.check:
        print(">>> Checking if models match stored spec...")

        if not STORED_SPEC.exists():
            print(f"ERROR: No stored spec at {STORED_SPEC}")
            print("Run: python scripts/generate_api_client.py --update-spec")
            sys.exit(1)

        gen_dir = _temp_dir / "generated"
        generate_models(STORED_SPEC, gen_dir)
        generate_models_init(gen_dir / "stepflow_api" / "models")
        format_generated_files(gen_dir / "stepflow_api" / "models")

        existing_models = API_CLIENT_DIR / "src" / "stepflow_api" / "models" / "generated.py"
        if not existing_models.exists():
            print(f"ERROR: No existing models at {existing_models}")
            print("Run: python scripts/generate_api_client.py")
            sys.exit(1)

        if not compare_models(gen_dir / "stepflow_api" / "models" / "generated.py", existing_models):
            print("ERROR: stepflow-api models are out of date!")
            print()
            print("To fix:")
            print("  1. If the Rust API changed: python scripts/generate_api_client.py --update-spec")
            print("  2. Regenerate models: python scripts/generate_api_client.py")
            sys.exit(1)

        print(">>> stepflow-api models are up-to-date")

    else:
        # Generate mode
        if args.from_spec:
            spec_file = args.from_spec
        elif STORED_SPEC.exists():
            spec_file = STORED_SPEC
            print(f">>> Using stored spec: {STORED_SPEC}")
        else:
            print(">>> No stored spec found, fetching from server...")
            spec_file = _temp_dir / "openapi.json"
            fetch_spec_from_server(spec_file)

        # Find custom model overrides before generating
        custom_overrides: list[str] = []
        existing_models = API_CLIENT_DIR / "src" / "stepflow_api" / "models"
        if existing_models.exists():
            for model_file in existing_models.glob("*.py"):
                if model_file.name in ("__init__.py", "generated.py"):
                    continue
                content = model_file.read_text()
                # Custom files don't start with the stub docstring pattern
                if not content.startswith('"""Re-export '):
                    custom_overrides.append(model_file.name)

        gen_dir = _temp_dir / "generated"
        generate_models(spec_file, gen_dir)
        generate_models_init(gen_dir / "stepflow_api" / "models", custom_overrides)
        generate_client_base(gen_dir)

        # Preserve existing API endpoint modules
        existing_api = API_CLIENT_DIR / "src" / "stepflow_api" / "api"
        if existing_api.exists():
            print(">>> Preserving existing API endpoint modules...")
            shutil.copytree(existing_api, gen_dir / "stepflow_api" / "api")

        # Copy custom model overrides
        if custom_overrides:
            print(">>> Preserving custom model overrides...")
            for name in custom_overrides:
                print(f"    Preserving {name}")
                shutil.copy(existing_models / name, gen_dir / "stepflow_api" / "models" / name)

        # Replace the package
        target_dir = API_CLIENT_DIR / "src" / "stepflow_api"
        if target_dir.exists():
            shutil.rmtree(target_dir)
        shutil.copytree(gen_dir / "stepflow_api", target_dir)

        # Format generated files
        print(">>> Formatting generated files...")
        format_generated_files(target_dir / "models")

        print(f">>> Client updated: {target_dir}")
        print()
        print("NOTE: Models regenerated with pydantic v2. API endpoint modules preserved.")

    print("=== Done ===")


if __name__ == "__main__":
    main()
