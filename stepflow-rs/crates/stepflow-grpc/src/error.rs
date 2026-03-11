// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

//! Structured gRPC error handling using the Richer Error Model.
//!
//! Maps domain errors to `tonic::Status` with structured error details
//! (via `tonic-types`) following the [gRPC Richer Error Model](https://grpc.io/docs/guides/error/).
//!
//! # Error categories
//!
//! | Domain error                 | gRPC code              | Detail type     |
//! |------------------------------|------------------------|-----------------|
//! | Validation / bad input       | `INVALID_ARGUMENT`     | `BadRequest`    |
//! | Resource not found           | `NOT_FOUND`            | `ErrorInfo`     |
//! | State conflict               | `FAILED_PRECONDITION`  | `ErrorInfo`     |
//! | Internal / infrastructure    | `INTERNAL`             | `DebugInfo`     |

use tonic::Status;
use tonic_types::{ErrorDetails, StatusExt as _};

/// Domain identifier for Stepflow errors in `ErrorInfo` details.
const ERROR_DOMAIN: &str = "stepflow.io";

/// Create an `INVALID_ARGUMENT` status.
pub fn invalid_argument(message: impl Into<String>) -> Status {
    let message = message.into();
    Status::new(tonic::Code::InvalidArgument, &message)
}

/// Create an `INVALID_ARGUMENT` status with a specific field violation.
pub fn invalid_field(field: impl Into<String>, description: impl Into<String>) -> Status {
    let field = field.into();
    let desc = description.into();
    let details = ErrorDetails::with_bad_request_violation(&field, &desc);
    Status::with_error_details(tonic::Code::InvalidArgument, &desc, details)
}

/// Create a `NOT_FOUND` status with error info.
pub fn not_found(resource_type: impl Into<String>, resource_id: impl Into<String>) -> Status {
    let rtype = resource_type.into();
    let rid = resource_id.into();
    let message = format!("{rtype} '{rid}' not found");
    let details = ErrorDetails::with_error_info(
        "RESOURCE_NOT_FOUND",
        ERROR_DOMAIN,
        [("resource_type".into(), rtype), ("resource_id".into(), rid)],
    );
    Status::with_error_details(tonic::Code::NotFound, &message, details)
}

/// Create a `FAILED_PRECONDITION` status with error info.
pub fn failed_precondition(reason: impl Into<String>, message: impl Into<String>) -> Status {
    let reason = reason.into();
    let message = message.into();
    let details = ErrorDetails::with_error_info(&reason, ERROR_DOMAIN, []);
    Status::with_error_details(tonic::Code::FailedPrecondition, &message, details)
}

/// Create an `INTERNAL` status with optional debug info.
///
/// In debug builds, the error chain from `error_stack::Report` is included
/// in the `DebugInfo` detail. In release builds, only the top-level message
/// is included.
pub fn internal(message: impl Into<String>) -> Status {
    let message = message.into();
    let details = ErrorDetails::with_debug_info(vec![], &message);
    Status::with_error_details(tonic::Code::Internal, &message, details)
}

/// Create an `INTERNAL` status from an `error_stack::Report`, preserving
/// the error chain in `DebugInfo`.
pub fn internal_from_report<C: error_stack::Context>(report: &error_stack::Report<C>) -> Status {
    let message = report.to_string();
    let stack_entries: Vec<String> = report
        .frames()
        .filter_map(|frame| {
            frame
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| frame.downcast_ref::<&str>().map(|s| s.to_string()))
        })
        .collect();
    let details = ErrorDetails::with_debug_info(stack_entries, &message);
    Status::with_error_details(tonic::Code::Internal, &message, details)
}

/// Convert `stepflow_execution::ExecutionError` to a gRPC status.
pub fn from_execution_error(
    report: &error_stack::Report<stepflow_execution::ExecutionError>,
) -> Status {
    use stepflow_execution::ExecutionError;

    let err = report.current_context();
    let message = report.to_string();

    match err {
        // Client errors → INVALID_ARGUMENT
        ExecutionError::OverrideError
        | ExecutionError::AnalysisError
        | ExecutionError::MalformedReference { .. }
        | ExecutionError::UndefinedValue(_)
        | ExecutionError::UndefinedField { .. } => {
            let details = ErrorDetails::with_bad_request(vec![]);
            Status::with_error_details(tonic::Code::InvalidArgument, &message, details)
        }

        // Not found → NOT_FOUND
        ExecutionError::ExecutionNotFound(id) => not_found("run", id.to_string()),
        ExecutionError::RunNotFound => {
            let details = ErrorDetails::with_error_info("RESOURCE_NOT_FOUND", ERROR_DOMAIN, []);
            Status::with_error_details(tonic::Code::NotFound, &message, details)
        }
        ExecutionError::WorkflowNotFound(id) => not_found("flow", id.to_string()),
        ExecutionError::BlobNotFound { .. } => {
            let details = ErrorDetails::with_error_info("RESOURCE_NOT_FOUND", ERROR_DOMAIN, []);
            Status::with_error_details(tonic::Code::NotFound, &message, details)
        }
        ExecutionError::StepNotFound { .. } => {
            let details = ErrorDetails::with_error_info("RESOURCE_NOT_FOUND", ERROR_DOMAIN, []);
            Status::with_error_details(tonic::Code::NotFound, &message, details)
        }

        // Everything else → INTERNAL with debug info
        _ => internal_from_report(report),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_field_has_bad_request_detail() {
        let status = invalid_field("flow_id", "flow_id is required");
        assert_eq!(status.code(), tonic::Code::InvalidArgument);

        let details = status.get_error_details();
        let br = details.bad_request().unwrap();
        assert_eq!(br.field_violations.len(), 1);
        assert_eq!(br.field_violations[0].field, "flow_id");
    }

    #[test]
    fn test_not_found_has_error_info() {
        let status = not_found("flow", "sha256:abc123");
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert!(status.message().contains("flow"));
        assert!(status.message().contains("sha256:abc123"));

        let details = status.get_error_details();
        let ei = details.error_info().unwrap();
        assert_eq!(ei.reason, "RESOURCE_NOT_FOUND");
        assert_eq!(ei.domain, ERROR_DOMAIN);
    }

    #[test]
    fn test_failed_precondition_has_error_info() {
        let status = failed_precondition("RUN_NOT_DELETABLE", "run is still running");
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);

        let details = status.get_error_details();
        let ei = details.error_info().unwrap();
        assert_eq!(ei.reason, "RUN_NOT_DELETABLE");
    }

    #[test]
    fn test_internal_has_debug_info() {
        let status = internal("database connection failed");
        assert_eq!(status.code(), tonic::Code::Internal);

        let details = status.get_error_details();
        let di = details.debug_info().unwrap();
        assert_eq!(di.detail, "database connection failed");
    }
}
