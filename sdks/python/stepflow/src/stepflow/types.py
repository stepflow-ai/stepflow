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

"""Shared types for Stepflow Python packages."""

from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Any


class FlowResultStatus(Enum):
    """Status of a flow execution result."""

    SUCCESS = "success"
    FAILED = "failed"
    SKIPPED = "skipped"


@dataclass
class FlowError:
    """Error information from a failed flow execution."""

    code: int
    message: str
    details: dict[str, Any] | None = None


@dataclass
class FlowResult:
    """Result of a flow execution.

    Represents the outcome of running a workflow, which can be:
    - Success with output data
    - Failed with error information
    - Skipped (conditionally not executed)
    """

    status: FlowResultStatus
    output: dict[str, Any] | None = None
    error: FlowError | None = None
    skipped_reason: str | None = None

    @classmethod
    def success(cls, output: dict[str, Any]) -> "FlowResult":
        """Create a successful result."""
        return cls(status=FlowResultStatus.SUCCESS, output=output)

    @classmethod
    def failed(
        cls, code: int, message: str, details: dict[str, Any] | None = None
    ) -> "FlowResult":
        """Create a failed result."""
        return cls(
            status=FlowResultStatus.FAILED,
            error=FlowError(code=code, message=message, details=details),
        )

    @classmethod
    def skipped(cls, reason: str | None = None) -> "FlowResult":
        """Create a skipped result."""
        return cls(status=FlowResultStatus.SKIPPED, skipped_reason=reason)

    @property
    def is_success(self) -> bool:
        """Check if the result is successful."""
        return self.status == FlowResultStatus.SUCCESS

    @property
    def is_failed(self) -> bool:
        """Check if the result is failed."""
        return self.status == FlowResultStatus.FAILED

    @property
    def is_skipped(self) -> bool:
        """Check if the result was skipped."""
        return self.status == FlowResultStatus.SKIPPED


@dataclass
class Diagnostic:
    """A single diagnostic message from validation."""

    level: str  # "error", "warning", "info"
    message: str
    location: str | None = None


@dataclass
class ValidationResult:
    """Result of validating a workflow.

    Contains diagnostics (errors, warnings, info) from the validation process.
    """

    valid: bool
    diagnostics: list[Diagnostic] = field(default_factory=list)

    @property
    def errors(self) -> list[Diagnostic]:
        """Get all error diagnostics."""
        return [d for d in self.diagnostics if d.level == "error"]

    @property
    def warnings(self) -> list[Diagnostic]:
        """Get all warning diagnostics."""
        return [d for d in self.diagnostics if d.level == "warning"]


@dataclass
class LogEntry:
    """A single log entry from the stepflow server."""

    timestamp: datetime
    level: str
    message: str
    target: str | None = None
    trace_id: str | None = None
    span_id: str | None = None
    run_id: str | None = None
    step_id: str | None = None
    extra: dict[str, Any] = field(default_factory=dict)


class RestartPolicy(Enum):
    """Policy for restarting the stepflow server subprocess."""

    NEVER = "never"
    """Never restart the server."""

    ON_FAILURE = "on_failure"
    """Restart the server if it exits with a non-zero exit code."""

    ALWAYS = "always"
    """Always restart the server when it exits."""


@dataclass
class ComponentInfo:
    """Information about an available component."""

    path: str
    description: str | None = None
    input_schema: dict[str, Any] | None = None
    output_schema: dict[str, Any] | None = None
