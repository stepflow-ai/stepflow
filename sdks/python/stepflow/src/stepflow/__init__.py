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

"""Stepflow shared types and interfaces.

This package provides common types and interfaces used by:
- stepflow-client: HTTP client for remote Stepflow servers
- stepflow-runtime: Embedded Stepflow runtime with bundled server
- stepflow-server: Component server SDK for building custom components

Example:
    ```python
    from stepflow import FlowResult, StepflowExecutor, RestartPolicy


    # Use the protocol for type hints
    async def run_workflow(executor: StepflowExecutor) -> FlowResult:
        return await executor.run("workflow.yaml", {"x": 1})
    ```
"""

from .interface import StepflowExecutor
from .types import (
    ComponentInfo,
    Diagnostic,
    FlowError,
    FlowResult,
    FlowResultStatus,
    LogEntry,
    RestartPolicy,
    ValidationResult,
)

__version__ = "0.1.0"

__all__ = [
    # Protocol
    "StepflowExecutor",
    # Types
    "ComponentInfo",
    "Diagnostic",
    "FlowError",
    "FlowResult",
    "FlowResultStatus",
    "LogEntry",
    "RestartPolicy",
    "ValidationResult",
]
