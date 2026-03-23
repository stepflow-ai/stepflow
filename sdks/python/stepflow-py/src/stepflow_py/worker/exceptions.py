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

import sys
from enum import IntEnum
from typing import Any

from stepflow_py.proto.common_pb2 import (
    TASK_ERROR_CODE_COMPONENT_FAILED,
    TASK_ERROR_CODE_COMPONENT_NOT_FOUND,
    TASK_ERROR_CODE_INVALID_INPUT,
    TASK_ERROR_CODE_RESOURCE_UNAVAILABLE,
    TASK_ERROR_CODE_WORKER_ERROR,
)


class ErrorCode(IntEnum):
    PARSE_ERROR = -32700
    JSON_RPC_INVALID_REQUEST = -32600
    METHOD_NOT_FOUND = -32601
    INVALID_PARAMS = -32602
    JSON_RPC_INTERNAL_ERROR = -32603
    WORKER_ERROR = -32000
    COMPONENT_NOT_FOUND = -32001
    WORKER_NOT_INITIALIZED = -32002
    INVALID_INPUT_SCHEMA = -32003
    INVALID_VALUE = -32004
    NOT_FOUND = -32005
    PROTOCOL_VERSION_MISMATCH = -32006
    WORKER_DEPENDENCY_ERROR = -32007
    WORKER_CONFIGURATION_ERROR = -32008
    COMPONENT_EXECUTION_FAILED = -32100
    COMPONENT_VALUE_ERROR = -32101
    COMPONENT_RESOURCE_UNAVAILABLE = -32102
    COMPONENT_BAD_REQUEST = -32103
    UNDEFINED_FIELD = -32200
    ENTITY_NOT_FOUND = -32201
    INTERNAL_ERROR = -32202
    TRANSPORT_ERROR = -32300
    TRANSPORT_SPAWN_ERROR = -32301
    TRANSPORT_CONNECTION_ERROR = -32302
    TRANSPORT_PROTOCOL_ERROR = -32303


def is_transport_error(code: int) -> bool:
    """Returns True if the given code represents a transport/infrastructure error."""
    return -32399 <= code <= -32300


def is_component_execution_error(code: int) -> bool:
    """Returns True if the code is a component execution error (retryable)."""
    return -32199 <= code <= -32100


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
        return ErrorCode.WORKER_ERROR

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

    def to_json_rpc_error(self) -> dict:
        """Convert to JSON-RPC error format."""
        return {"code": self.code.value, "message": self.message, "data": self.data}


class StepflowProtocolError(StepflowError):
    """Errors related to JSON-RPC protocol violations."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.JSON_RPC_INVALID_REQUEST

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_WORKER_ERROR


class StepflowComponentError(StepflowError):
    """Errors related to component operations."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.COMPONENT_NOT_FOUND

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_COMPONENT_NOT_FOUND


class StepflowValidationError(StepflowError):
    """Errors related to input/output validation."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.INVALID_INPUT_SCHEMA

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_INVALID_INPUT


class StepflowValueError(StepflowError):
    """Errors related to invalid values."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.COMPONENT_VALUE_ERROR

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_INVALID_INPUT


class StepflowExecutionError(StepflowError):
    """Errors during component execution."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.COMPONENT_EXECUTION_FAILED

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_COMPONENT_FAILED


class StepflowRuntimeError(StepflowError):
    """Errors from the Stepflow runtime."""

    @property
    def default_code(self) -> ErrorCode:
        return ErrorCode.COMPONENT_RESOURCE_UNAVAILABLE

    @property
    def task_error_code(self) -> int:
        return TASK_ERROR_CODE_RESOURCE_UNAVAILABLE


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
        return ErrorCode.WORKER_NOT_INITIALIZED


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
