use axum::http::StatusCode;
use axum::response::IntoResponse;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::FlowHash;
use uuid::Uuid;

/// Error response structure.
///
/// Server handlers should return this, but usually it is better to create it
/// by returning an `error_stack::Report<ServerError>` and using the automatic
/// conversion to `ErrorResponse`.
///
/// Other `error_stack::Report` types will automatically convert to internal errors.
#[derive(Debug, serde::Serialize)]
pub struct ErrorResponse {
    #[serde(serialize_with = "serialize_status_code")]
    pub code: StatusCode,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Execution '{0}' not found")]
    ExecutionNotFound(Uuid),
    #[error("Workflow '{0}' not found")]
    WorkflowNotFound(FlowHash),
    #[error("Execution '{execution_id}' cannot be cancelled (status: {status:?})")]
    ExecutionNotCancellable {
        execution_id: Uuid,
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

        ErrorResponse {
            code,
            message: report.to_string(),
        }
    }
}
