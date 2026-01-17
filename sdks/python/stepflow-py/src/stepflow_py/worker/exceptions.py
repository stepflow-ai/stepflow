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

from enum import IntEnum
from typing import Any


class ErrorCode(IntEnum):
    GENERIC_ERROR = -32000
    NOT_INITIALIZED = -32001

    INVALID_REQUEST = -32600
    COMPONENT_ERROR = -32001
    VALIDATION_ERROR = -32002
    EXECUTION_ERROR = -32003
    RUNTIME_ERROR = -32004
    VALUE_ERROR = -32005


class StepflowError(Exception):
    """Base exception for all Stepflow SDK errors."""

    def __init__(self, message: str, code: ErrorCode = None, data: dict = None):
        super().__init__(message)
        self.message = message
        self.code = code or self.default_code
        self.data = data or {}

    @property
    def default_code(self) -> ErrorCode:
        """Default error code for this exception type."""
        return ErrorCode.GENERIC_ERROR

    def to_json_rpc_error(self) -> dict:
        """Convert to JSON-RPC error format."""
        return {"code": self.code.value, "message": self.message, "data": self.data}


class StepflowProtocolError(StepflowError):
    """Errors related to JSON-RPC protocol violations."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.INVALID_REQUEST  # Invalid Request


class StepflowComponentError(StepflowError):
    """Errors related to component operations."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.COMPONENT_ERROR


class StepflowValidationError(StepflowError):
    """Errors related to input/output validation."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.VALIDATION_ERROR


class StepflowValueError(StepflowError):
    """Errors related to invalid values."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.VALUE_ERROR


class StepflowExecutionError(StepflowError):
    """Errors during component execution."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.EXECUTION_ERROR


class StepflowRuntimeError(StepflowError):
    """Errors from the Stepflow runtime."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.RUNTIME_ERROR


class ComponentNotFoundError(StepflowComponentError):
    """Component was not found."""

    def __init__(self, component_name: str):
        super().__init__(f"Component '{component_name}' not found")
        self.component_name = component_name
        self.data = {"component": component_name}


class ServerNotInitializedError(StepflowProtocolError):
    """Server hasn't been initialized yet."""

    def __init__(self):
        super().__init__("Server not initialized")

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.NOT_INITIALIZED


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

    def __init__(self, error_code: int, message: str, data: Any = None):
        super().__init__(message)
        self.error_code = error_code
        self.message = message
        self.data = data
