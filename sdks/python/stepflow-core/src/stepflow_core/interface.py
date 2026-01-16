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

"""Protocol definitions for Stepflow executors.

This module defines the common interface that both StepflowClient (remote)
and StepflowRuntime (local) implement, allowing them to be used interchangeably.
"""

from pathlib import Path
from typing import Any, Protocol, runtime_checkable

from .types import ComponentInfo, FlowResult, ValidationResult


@runtime_checkable
class StepflowExecutor(Protocol):
    """Protocol for Stepflow execution backends.

    Both StepflowClient (for remote servers) and StepflowRuntime (for local
    embedded execution) implement this protocol, allowing code to work with
    either backend interchangeably.

    Example:
        ```python
        def get_executor(local: bool = True) -> StepflowExecutor:
            if local:
                from stepflow_runtime import StepflowRuntime

                return StepflowRuntime.start("config.yml")
            from stepflow_client import StepflowClient

            return StepflowClient("http://production:7837")


        executor = get_executor(local=True)
        result = await executor.run("workflow.yaml", {"x": 1})
        ```
    """

    @property
    def url(self) -> str:
        """Base URL of the stepflow server.

        For StepflowClient, this is the remote server URL.
        For StepflowRuntime, this is the local server URL (e.g., http://localhost:7840).
        """
        ...

    async def run(
        self,
        flow: str | Path | dict[str, Any],
        input: dict[str, Any],
        overrides: dict[str, Any] | None = None,
    ) -> FlowResult:
        """Run a workflow and wait for the result.

        Args:
            flow: Path to workflow file, or workflow dict
            input: Input data for the workflow
            overrides: Optional step overrides

        Returns:
            FlowResult with success/failure status and output
        """
        ...

    async def submit(
        self,
        flow: str | Path | dict[str, Any],
        input: dict[str, Any],
        overrides: dict[str, Any] | None = None,
    ) -> str:
        """Submit a workflow for execution and return immediately.

        Args:
            flow: Path to workflow file, or workflow dict
            input: Input data for the workflow
            overrides: Optional step overrides

        Returns:
            Run ID for tracking the execution
        """
        ...

    async def get_result(self, run_id: str) -> FlowResult:
        """Get the result of a submitted workflow run.

        Args:
            run_id: The run ID returned from submit()

        Returns:
            FlowResult with the execution outcome
        """
        ...

    async def validate(
        self,
        flow: str | Path | dict[str, Any],
    ) -> ValidationResult:
        """Validate a workflow without executing it.

        Args:
            flow: Path to workflow file, or workflow dict

        Returns:
            ValidationResult with diagnostics
        """
        ...

    async def list_components(self) -> list[ComponentInfo]:
        """List all available components.

        Returns:
            List of ComponentInfo with component details
        """
        ...
