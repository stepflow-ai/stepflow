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

from typing import Any

from stepflow_py.proto.common_pb2 import (
    TASK_ERROR_CODE_COMPONENT_FAILED,
    TASK_ERROR_CODE_COMPONENT_NOT_FOUND,
    TASK_ERROR_CODE_INVALID_INPUT,
    TASK_ERROR_CODE_RESOURCE_UNAVAILABLE,
    TASK_ERROR_CODE_WORKER_ERROR,
)


class StepflowError(Exception):
    """Base exception for all Stepflow SDK errors."""

    def __init__(self, message: str, data: dict | None = None):
        super().__init__(message)
        self.message = message
        self.data = data or {}

    @property
    def task_error_code(self) -> int:
        """Proto TaskErrorCode for this exception. Subclasses override."""
        return TASK_ERROR_CODE_WORKER_ERROR

    def task_error_data(self) -> dict:
        """Structured error data to include in TaskError.data."""
        result: dict[str, Any] = {"exception_type": type(self).__name__}
        if self.data:
            result["details"] = self.data
        return result


class StepflowComponentError(StepflowError):
    """Errors related to component operations."""

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_COMPONENT_NOT_FOUND


class StepflowValidationError(StepflowError):
    """Errors related to input/output validation."""

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_INVALID_INPUT


class StepflowValueError(StepflowError):
    """Errors related to invalid values."""

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_INVALID_INPUT


class StepflowExecutionError(StepflowError):
    """Errors during component execution."""

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_COMPONENT_FAILED


class StepflowRuntimeError(StepflowError):
    """Errors from the Stepflow runtime."""

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_RESOURCE_UNAVAILABLE


class ComponentNotFoundError(StepflowComponentError):
    """Component was not found."""

    def __init__(self, component_name: str):
        super().__init__(f"Component '{component_name}' not found")
        self.component_name = component_name
        self.data = {"component": component_name}


class InputValidationError(StepflowValidationError):
    """Input validation failed."""

    def __init__(self, validation_error: str, input_data: dict | None = None):
        super().__init__(f"Input validation failed: {validation_error}")
        if input_data:
            self.data = {"input": input_data}


class BlobNotFoundError(StepflowRuntimeError):
    """Blob was not found in storage."""

    def __init__(self, blob_id: str):
        super().__init__(f"Blob '{blob_id}' not found")
        self.blob_id = blob_id
        self.data = {"blob_id": blob_id}


class CodeCompilationError(StepflowExecutionError):
    """User code compilation failed."""

    def __init__(self, compilation_error: str, code: str | None = None):
        super().__init__(f"Code compilation failed: {compilation_error}")
        if code:
            self.data = {"code": code}


class StepflowFailed(Exception):
    """Exception raised when a step or flow fails with a business logic error."""

    def __init__(self, error_code: int | str, message: str, data: Any = None):
        super().__init__(message)
        self.error_code = error_code
        self.message = message
        self.data = data
