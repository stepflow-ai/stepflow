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

"""Embedded Stepflow runtime with bundled server binary.

This package provides an embedded Stepflow server that manages a subprocess
lifecycle, enabling local workflow execution without requiring a separate
server installation.

Example:
    ```python
    from stepflow_runtime import StepflowRuntime

    # Basic usage - starts with builtin plugin only
    async with StepflowRuntime.start() as runtime:
        result = await runtime.run("workflow.yaml", {"x": 1})
        if result.is_success:
            print(f"Output: {result.output}")

    # With configuration file
    async with StepflowRuntime.start("stepflow-config.yml") as runtime:
        result = await runtime.run("workflow.yaml", {"x": 1})
    ```

The runtime implements the StepflowExecutor protocol, making it interchangeable
with StepflowClient for remote server access.
"""

from stepflow_core import (
    ComponentInfo,
    FlowError,
    FlowResult,
    FlowResultStatus,
    LogEntry,
    RestartPolicy,
    ValidationResult,
)

from .logging import LogConfig, LogForwarder
from .runtime import StepflowRuntime, StepflowRuntimeError
from .utils import find_free_port, get_binary_path, is_port_in_use

__version__ = "0.1.0"

__all__ = [
    # Main runtime
    "StepflowRuntime",
    "StepflowRuntimeError",
    # Configuration
    "LogConfig",
    "RestartPolicy",
    # Utilities
    "get_binary_path",
    "find_free_port",
    "is_port_in_use",
    "LogForwarder",
    # Re-exported types from stepflow-core
    "FlowResult",
    "FlowResultStatus",
    "FlowError",
    "ValidationResult",
    "ComponentInfo",
    "LogEntry",
]
