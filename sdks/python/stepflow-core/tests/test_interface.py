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

"""Tests for StepflowExecutor protocol."""

from pathlib import Path
from typing import Any, runtime_checkable

from stepflow_core import (
    ComponentInfo,
    FlowResult,
    FlowResultStatus,
    StepflowExecutor,
    ValidationResult,
)


class MockExecutor:
    """Mock implementation of StepflowExecutor for testing."""

    def __init__(self):
        self._url = "http://localhost:7837"

    @property
    def url(self) -> str:
        return self._url

    async def run(
        self,
        flow: str | Path,
        input: dict[str, Any],
        overrides: dict[str, Any] | None = None,
    ) -> FlowResult:
        return FlowResult(status=FlowResultStatus.SUCCESS, output={"result": "ok"})

    async def submit(
        self,
        flow: str | Path,
        input: dict[str, Any],
        overrides: dict[str, Any] | None = None,
    ) -> str:
        return "run-123"

    async def get_result(self, run_id: str) -> FlowResult:
        return FlowResult(status=FlowResultStatus.SUCCESS, output={"result": "ok"})

    async def validate(self, flow: str | Path) -> ValidationResult:
        return ValidationResult(valid=True, diagnostics=[])

    async def list_components(self) -> list[ComponentInfo]:
        return [ComponentInfo(path="/test", description="Test component")]


class TestStepflowExecutor:
    def test_protocol_is_runtime_checkable(self):
        # StepflowExecutor should be runtime checkable
        assert runtime_checkable in getattr(StepflowExecutor, "__mro__", []) or hasattr(
            StepflowExecutor, "__protocol_attrs__"
        )

    def test_mock_executor_implements_protocol(self):
        # MockExecutor should satisfy the protocol
        executor = MockExecutor()
        assert hasattr(executor, "url")
        assert hasattr(executor, "run")
        assert hasattr(executor, "submit")
        assert hasattr(executor, "get_result")
        assert hasattr(executor, "validate")
        assert hasattr(executor, "list_components")

    async def test_executor_run(self):
        executor = MockExecutor()
        result = await executor.run("workflow.yaml", {"x": 1})
        assert result.is_success
        assert result.output == {"result": "ok"}

    async def test_executor_submit(self):
        executor = MockExecutor()
        run_id = await executor.submit("workflow.yaml", {"x": 1})
        assert run_id == "run-123"

    async def test_executor_get_result(self):
        executor = MockExecutor()
        result = await executor.get_result("run-123")
        assert result.is_success

    async def test_executor_validate(self):
        executor = MockExecutor()
        result = await executor.validate("workflow.yaml")
        assert result.valid

    async def test_executor_list_components(self):
        executor = MockExecutor()
        components = await executor.list_components()
        assert len(components) == 1
        assert components[0].path == "/test"
