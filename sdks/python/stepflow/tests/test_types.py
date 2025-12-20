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

"""Tests for shared types."""

from datetime import datetime

from stepflow import (
    ComponentInfo,
    Diagnostic,
    FlowError,
    FlowResult,
    FlowResultStatus,
    LogEntry,
    RestartPolicy,
    ValidationResult,
)


class TestFlowResult:
    def test_success_result(self):
        result = FlowResult(
            status=FlowResultStatus.SUCCESS,
            output={"result": 42},
        )
        assert result.is_success
        assert not result.is_failed
        assert not result.is_skipped
        assert result.output == {"result": 42}
        assert result.error is None

    def test_failed_result(self):
        error = FlowError(code=400, message="Bad request")
        result = FlowResult(
            status=FlowResultStatus.FAILED,
            error=error,
        )
        assert not result.is_success
        assert result.is_failed
        assert not result.is_skipped
        assert result.output is None
        assert result.error.code == 400
        assert result.error.message == "Bad request"

    def test_skipped_result(self):
        result = FlowResult(status=FlowResultStatus.SKIPPED)
        assert not result.is_success
        assert not result.is_failed
        assert result.is_skipped


class TestFlowError:
    def test_error_creation(self):
        error = FlowError(code=500, message="Internal error", details={"detail": "oops"})
        assert error.code == 500
        assert error.message == "Internal error"
        assert error.details == {"detail": "oops"}


class TestValidationResult:
    def test_valid_result(self):
        result = ValidationResult(valid=True, diagnostics=[])
        assert result.valid
        assert len(result.diagnostics) == 0

    def test_invalid_result(self):
        diagnostics = [
            Diagnostic(
                level="error",
                message="Missing required field",
                location="$.input.name",
            )
        ]
        result = ValidationResult(valid=False, diagnostics=diagnostics)
        assert not result.valid
        assert len(result.diagnostics) == 1
        assert result.diagnostics[0].level == "error"


class TestLogEntry:
    def test_log_entry_creation(self):
        now = datetime.now()
        entry = LogEntry(
            timestamp=now,
            level="info",
            message="Test message",
            target="test.module",
            trace_id="abc123",
        )
        assert entry.timestamp == now
        assert entry.level == "info"
        assert entry.message == "Test message"
        assert entry.target == "test.module"
        assert entry.trace_id == "abc123"


class TestRestartPolicy:
    def test_policy_values(self):
        assert RestartPolicy.NEVER.value == "never"
        assert RestartPolicy.ON_FAILURE.value == "on_failure"
        assert RestartPolicy.ALWAYS.value == "always"


class TestComponentInfo:
    def test_component_info_creation(self):
        info = ComponentInfo(
            path="/my/component",
            description="A test component",
            input_schema={"type": "object"},
            output_schema={"type": "string"},
        )
        assert info.path == "/my/component"
        assert info.description == "A test component"
        assert info.input_schema == {"type": "object"}
        assert info.output_schema == {"type": "string"}
