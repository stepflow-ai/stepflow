use axum::http::StatusCode;
use axum::response::IntoResponse;
use stepflow_core::workflow::FlowHash;
use uuid::Uuid;

/// Error response structure.
#[derive(Debug, serde::Serialize)]
pub struct ErrorResponse {
    #[serde(serialize_with = "serialize_status_code")]
    pub code: StatusCode,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Invalid execution ID '{0}'")]
    InvalidExecutionId(String),
    #[error("Execution '{0}' not found")]
    ExecutionNotFound(Uuid),
    #[error("Endpoint '{0}' not found")]
    EndpointNotFound(String),
    #[error("Endpoint '{name}' does not have label {label:?}")]
    EndpointLabelNotFound { name: String, label: Option<String> },
    #[error("Workflow '{0}' not found")]
    WorkflowNotFound(FlowHash),
}

impl ServerError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ServerError::InvalidExecutionId(_) => StatusCode::BAD_REQUEST,
            ServerError::ExecutionNotFound(_)
            | ServerError::EndpointNotFound(_)
            | ServerError::EndpointLabelNotFound { .. }
            | ServerError::WorkflowNotFound(_) => StatusCode::NOT_FOUND,
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
