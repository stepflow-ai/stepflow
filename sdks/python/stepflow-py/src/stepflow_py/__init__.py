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

"""Stepflow Python SDK - build and run AI workflows.

This is the unified Python SDK for Stepflow. It re-exports the key classes from
the individual packages for convenient access.

Example:
    ```python
    from stepflow_py import StepflowClient, FlowResult

    async with StepflowClient("http://localhost:7837") as client:
        result = await client.run("workflow.yaml", {"x": 1})
        if result.is_success:
            print(result.output)
    ```

For building components:
    ```python
    from stepflow_py import StepflowServer, StepflowContext
    import msgspec


    class Input(msgspec.Struct):
        message: str


    class Output(msgspec.Struct):
        result: str


    server = StepflowServer()


    @server.component
    def greet(input: Input) -> Output:
        return Output(result=f"Hello, {input.message}!")


    server.run()
    ```

For embedded runtime (requires `pip install stepflow-py[runtime]`):
    ```python
    from stepflow_py import StepflowRuntime

    async with StepflowRuntime.start() as runtime:
        result = await runtime.run("workflow.yaml", {"x": 1})
    ```
"""

__version__ = "0.1.0"

# =============================================================================
# Core types (from stepflow-core)
# =============================================================================
# =============================================================================
# HTTP Client (from stepflow-client)
# =============================================================================
from stepflow_client import (
    StepflowClient,
    StepflowClientError,
)
from stepflow_core import (
    ComponentInfo,
    Diagnostic,
    FlowError,
    FlowResult,
    FlowResultStatus,
    StepflowExecutor,
    ValidationResult,
)

# =============================================================================
# Component Server SDK (from stepflow-worker)
# =============================================================================
from stepflow_worker import (
    FlowBuilder,
    StepflowContext,
    StepflowServer,
    StepflowStdioServer,
    Value,
)

# =============================================================================
# Embedded Runtime (from stepflow-runtime, optional)
# =============================================================================
# StepflowRuntime requires the bundled binary, so it's optional.
# Install with: pip install stepflow-py[runtime]
try:
    from stepflow_runtime import (
        StepflowRuntime,  # noqa: F401
        StepflowRuntimeError,  # noqa: F401
    )

    _HAS_RUNTIME = True
except ImportError:
    _HAS_RUNTIME = False

# =============================================================================
# Public API
# =============================================================================
__all__ = [
    # Protocol
    "StepflowExecutor",
    # Client
    "StepflowClient",
    "StepflowClientError",
    # Component Server
    "StepflowServer",
    "StepflowStdioServer",
    "StepflowContext",
    # Workflow Building
    "FlowBuilder",
    "Value",
    # Result Types
    "FlowResult",
    "FlowResultStatus",
    "FlowError",
    "ValidationResult",
    "Diagnostic",
    "ComponentInfo",
]

# Add runtime exports if available
if _HAS_RUNTIME:
    __all__.extend(
        [
            "StepflowRuntime",
            "StepflowRuntimeError",
        ]
    )
