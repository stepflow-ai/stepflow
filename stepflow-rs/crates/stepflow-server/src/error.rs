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

use axum::http::StatusCode;
use axum::response::IntoResponse;
use stepflow_core::BlobId;
use stepflow_core::status::ExecutionStatus;
use uuid::Uuid;

/// Error response structure.
///
/// Server handlers should return this, but usually it is better to create it
/// by returning an `error_stack::Report<ServerError>` and using the automatic
/// conversion to `ErrorResponse`.
///
/// Other `error_stack::Report` types will automatically convert to internal errors.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ErrorResponse {
    #[serde(serialize_with = "serialize_status_code", deserialize_with = "deserialize_status_code")]
    pub code: StatusCode,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub stack: Vec<ErrorStackEntry>,
}

/// A single entry in the error stack
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ErrorStackEntry {
    pub error: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attachments: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backtrace: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Execution '{0}' not found")]
    ExecutionNotFound(Uuid),
    #[error("Workflow '{0}' not found")]
    WorkflowNotFound(BlobId),
    #[error("Run '{run_id}' cannot be cancelled (status: {status:?})")]
    ExecutionNotCancellable {
        run_id: Uuid,
        status: ExecutionStatus,
    },
    #[error("Execution '{0}' is still running and cannot be deleted")]
    ExecutionStillRunning(Uuid),
}

impl ServerError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ServerError::ExecutionNotFound(_) | ServerError::WorkflowNotFound(_) => {
                StatusCode::NOT_FOUND
            }
            ServerError::ExecutionNotCancellable { .. } | ServerError::ExecutionStillRunning(_) => {
                StatusCode::CONFLICT
            }
        }
    }
}

fn serialize_status_code<S>(code: &StatusCode, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_u16(code.as_u16())
}

fn deserialize_status_code<'de, D>(d: D) -> Result<StatusCode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let code = u16::deserialize(d)?;
    StatusCode::from_u16(code).map_err(serde::de::Error::custom)
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        let body = serde_json::to_string(&self).unwrap();
        (self.code, body).into_response()
    }
}

impl From<ServerError> for ErrorResponse {
    fn from(value: ServerError) -> Self {
        ErrorResponse {
            code: value.status_code(),
            message: value.to_string(),
            stack: vec![],
        }
    }
}

impl<T: error_stack::Context> From<error_stack::Report<T>> for ErrorResponse {
    fn from(report: error_stack::Report<T>) -> ErrorResponse {
        let code = report
            .downcast_ref()
            .cloned()
            .or_else(|| {
                report
                    .downcast_ref::<ServerError>()
                    .map(ServerError::status_code)
            })
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        // Build the error stack
        let mut stack: Vec<ErrorStackEntry> = Vec::new();
        let mut current_attachments: Vec<String> = Vec::new();

        // Extract global backtrace if available
        let global_backtrace = {
            let mut backtrace_iter = report.frames()
                .into_iter()
                .filter_map(|frame| {
                    frame.sources()
                        .into_iter()
                        .find_map(|source| source.downcast_ref::<std::backtrace::Backtrace>())
                });
            backtrace_iter.next().map(|bt| bt.to_string())
        };

        for frame in report.frames() {
            match frame.kind() {
                error_stack::FrameKind::Context(context) => {
                    // If we have accumulated attachments, add them to the previous entry
                    if !current_attachments.is_empty() && !stack.is_empty() {
                        if let Some(last_entry) = stack.last_mut() {
                            last_entry.attachments.extend(current_attachments.drain(..));
                        }
                    }

                    // Add the context as a new stack entry
                    // Only include backtrace on the first (top-level) error to avoid duplication
                    let backtrace = if stack.is_empty() { global_backtrace.clone() } else { None };

                    stack.push(ErrorStackEntry {
                        error: context.to_string(),
                        attachments: vec![],
                        backtrace,
                    });
                }
                error_stack::FrameKind::Attachment(attachment_kind) => {
                    match attachment_kind {
                        error_stack::AttachmentKind::Printable(printable) => {
                            current_attachments.push(printable.to_string());
                        }
                        error_stack::AttachmentKind::Opaque(opaque) => {
                            current_attachments.push(format!("<opaque attachment: {:p}>", opaque));
                        }
                        _ => {
                            current_attachments.push("<unknown attachment>".to_string());
                        }
                    }
                }
            }
        }

        // Add any remaining attachments to the last entry
        if !current_attachments.is_empty() && !stack.is_empty() {
            if let Some(last_entry) = stack.last_mut() {
                last_entry.attachments.extend(current_attachments);
            }
        }

        ErrorResponse {
            code,
            message: report.to_string(),
            stack,
        }
    }
}
