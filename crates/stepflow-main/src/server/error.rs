use error_stack::Report;
use poem::http::StatusCode;
use thiserror::Error;
use uuid::Uuid;

use stepflow_execution::ExecutionError;
use stepflow_plugin::PluginError;
use stepflow_state::StateError;

use crate::error::MainError;

/// Central error type for all server operations
#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Invalid request parameter: {message}")]
    InvalidParameter { message: String },

    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    #[error("Workflow storage failed")]
    WorkflowStoreFailed,

    #[error("Input data storage failed")]
    InputStoreFailed,

    #[error("Execution record creation failed")]
    ExecutionRecordFailed,

    #[error("Workflow submission failed")]
    WorkflowSubmissionFailed,

    #[error("Result retrieval failed")]
    ResultRetrievalFailed,

    #[error("Endpoint creation failed")]
    EndpointCreationFailed,

    #[error("Endpoint deletion failed")]
    EndpointDeletionFailed,

    #[error("Debug session creation failed")]
    DebugSessionCreationFailed,

    #[error("Debug session not found: {session_id}")]
    DebugSessionNotFound { session_id: Uuid },

    #[error("Debug step execution failed")]
    DebugStepExecutionFailed,

    #[error("Debug continue failed")]
    DebugContinueFailed,

    #[error("Component listing failed from plugin: {plugin_name}")]
    ComponentListingFailed { plugin_name: String },

    #[error("Component info retrieval failed for '{component}' from plugin '{plugin_name}'")]
    ComponentInfoFailed {
        component: String,
        plugin_name: String,
    },

    #[error("Invalid UUID format: {uuid_string}")]
    InvalidUuid { uuid_string: String },

    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

    #[error("State store operation failed")]
    StateStore,

    #[error("Execution engine operation failed")]
    Execution,

    #[error("Plugin operation failed")]
    Plugin,

    #[error("Main operation failed")]
    Main,
}

/// Transparent wrapper around error_stack::Report<ServerError> that implements poem's ResponseError
#[derive(Debug)]
pub struct ServerErrorReport(pub Report<ServerError>);

impl std::fmt::Display for ServerErrorReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ServerErrorReport {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None // error-stack::Report doesn't implement std::error::Error
    }
}

impl From<ServerErrorReport> for poem::Error {
    fn from(err: ServerErrorReport) -> Self {
        let status = match err.0.current_context() {
            ServerError::InvalidParameter { .. } => StatusCode::BAD_REQUEST,
            ServerError::InvalidUuid { .. } => StatusCode::BAD_REQUEST,
            ServerError::NotFound { .. } => StatusCode::NOT_FOUND,
            ServerError::DebugSessionNotFound { .. } => StatusCode::NOT_FOUND,
            ServerError::NotImplemented { .. } => StatusCode::NOT_IMPLEMENTED,
            ServerError::WorkflowStoreFailed
            | ServerError::InputStoreFailed
            | ServerError::ExecutionRecordFailed
            | ServerError::WorkflowSubmissionFailed
            | ServerError::ResultRetrievalFailed
            | ServerError::EndpointCreationFailed
            | ServerError::EndpointDeletionFailed
            | ServerError::DebugSessionCreationFailed
            | ServerError::DebugStepExecutionFailed
            | ServerError::DebugContinueFailed
            | ServerError::ComponentListingFailed { .. }
            | ServerError::ComponentInfoFailed { .. }
            | ServerError::StateStore
            | ServerError::Execution
            | ServerError::Plugin
            | ServerError::Main => StatusCode::INTERNAL_SERVER_ERROR,
        };

        // Log the full error context for debugging
        tracing::error!("Server error: {:?}", err.0);

        // Create the error response
        let mut error = poem::Error::from_status(status);
        error.set_error_message(err.0.to_string());
        error
    }
}

impl From<Report<ServerError>> for ServerErrorReport {
    fn from(report: Report<ServerError>) -> Self {
        Self(report)
    }
}

impl ServerErrorReport {
    /// Get the HTTP status code for this error (for testing)
    pub fn status(&self) -> StatusCode {
        match self.0.current_context() {
            ServerError::InvalidParameter { .. } => StatusCode::BAD_REQUEST,
            ServerError::InvalidUuid { .. } => StatusCode::BAD_REQUEST,
            ServerError::NotFound { .. } => StatusCode::NOT_FOUND,
            ServerError::DebugSessionNotFound { .. } => StatusCode::NOT_FOUND,
            ServerError::NotImplemented { .. } => StatusCode::NOT_IMPLEMENTED,
            ServerError::WorkflowStoreFailed
            | ServerError::InputStoreFailed
            | ServerError::ExecutionRecordFailed
            | ServerError::WorkflowSubmissionFailed
            | ServerError::ResultRetrievalFailed
            | ServerError::EndpointCreationFailed
            | ServerError::EndpointDeletionFailed
            | ServerError::DebugSessionCreationFailed
            | ServerError::DebugStepExecutionFailed
            | ServerError::DebugContinueFailed
            | ServerError::ComponentListingFailed { .. }
            | ServerError::ComponentInfoFailed { .. }
            | ServerError::StateStore
            | ServerError::Execution
            | ServerError::Plugin
            | ServerError::Main => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<Report<StateError>> for ServerErrorReport {
    fn from(report: Report<StateError>) -> Self {
        let server_error = match report.current_context() {
            StateError::BlobNotFound { blob_id } => ServerError::NotFound {
                resource: format!("blob {}", blob_id),
            },
            StateError::WorkflowNotFound { workflow_hash } => ServerError::NotFound {
                resource: format!("workflow {}", workflow_hash),
            },
            StateError::EndpointNotFound { name } => ServerError::NotFound {
                resource: format!("endpoint {}", name),
            },
            StateError::ExecutionNotFound { execution_id } => ServerError::NotFound {
                resource: format!("execution {}", execution_id),
            },
            _ => ServerError::StateStore,
        };

        Self(
            Report::new(server_error)
                .attach_printable("State store error")
                .attach(report),
        )
    }
}

impl From<Report<ExecutionError>> for ServerErrorReport {
    fn from(report: Report<ExecutionError>) -> Self {
        let server_error = match report.current_context() {
            ExecutionError::BlobNotFound { blob_id } => ServerError::NotFound {
                resource: format!("blob {}", blob_id),
            },
            ExecutionError::StepNotFound { step } => ServerError::NotFound {
                resource: format!("step {}", step),
            },
            _ => ServerError::Execution,
        };

        Self(
            Report::new(server_error)
                .attach_printable("Execution error")
                .attach(report),
        )
    }
}

impl From<Report<PluginError>> for ServerErrorReport {
    fn from(report: Report<PluginError>) -> Self {
        Self(
            Report::new(ServerError::Plugin)
                .attach_printable("Plugin error")
                .attach(report),
        )
    }
}

impl From<Report<MainError>> for ServerErrorReport {
    fn from(report: Report<MainError>) -> Self {
        Self(
            Report::new(ServerError::Main)
                .attach_printable("Main error")
                .attach(report),
        )
    }
}

/// Convenience type alias for server results  
/// poem-openapi requires Result<T, poem::Error> for endpoint return types
pub type ServerResult<T> = Result<T, poem::Error>;

/// Helper function to create a "not found" error
pub fn not_found(resource: impl Into<String>) -> poem::Error {
    ServerErrorReport(Report::new(ServerError::NotFound {
        resource: resource.into(),
    }))
    .into()
}

/// Helper function to create an "invalid parameter" error
pub fn invalid_parameter(message: impl Into<String>) -> poem::Error {
    ServerErrorReport(Report::new(ServerError::InvalidParameter {
        message: message.into(),
    }))
    .into()
}

/// Helper function to create an "invalid UUID" error
pub fn invalid_uuid(uuid_string: impl Into<String>) -> poem::Error {
    ServerErrorReport(Report::new(ServerError::InvalidUuid {
        uuid_string: uuid_string.into(),
    }))
    .into()
}

/// Helper function to create a "not implemented" error
pub fn not_implemented(feature: impl Into<String>) -> poem::Error {
    ServerErrorReport(Report::new(ServerError::NotImplemented {
        feature: feature.into(),
    }))
    .into()
}

/// Helper function to create a "bad request" error
pub fn bad_request(message: impl Into<String>) -> poem::Error {
    ServerErrorReport(Report::new(ServerError::InvalidParameter {
        message: message.into(),
    }))
    .into()
}

/// Helper trait to convert error-stack Reports into poem::Error
/// This avoids orphan rule issues while providing clean error conversion
pub trait IntoPoemError {
    type Output;
    fn into_poem(self) -> Self::Output;
}

impl IntoPoemError for Report<ServerError> {
    type Output = poem::Error;
    fn into_poem(self) -> Self::Output {
        ServerErrorReport(self).into()
    }
}

// Implementations for specific error types
impl IntoPoemError for Report<StateError> {
    type Output = poem::Error;
    fn into_poem(self) -> Self::Output {
        ServerErrorReport::from(self).into()
    }
}

impl IntoPoemError for Report<ExecutionError> {
    type Output = poem::Error;
    fn into_poem(self) -> Self::Output {
        ServerErrorReport::from(self).into()
    }
}

impl IntoPoemError for Report<PluginError> {
    type Output = poem::Error;
    fn into_poem(self) -> Self::Output {
        ServerErrorReport::from(self).into()
    }
}

impl IntoPoemError for Report<MainError> {
    type Output = poem::Error;
    fn into_poem(self) -> Self::Output {
        ServerErrorReport::from(self).into()
    }
}

// Implementation for Result types - this allows calling .into_poem()? directly on Results
impl<T, E> IntoPoemError for Result<T, E>
where
    E: IntoPoemError<Output = poem::Error>,
{
    type Output = Result<T, poem::Error>;
    fn into_poem(self) -> Self::Output {
        self.map_err(|err| err.into_poem())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use error_stack::Report;

    #[test]
    fn test_server_error_status_codes() {
        let invalid_param = ServerErrorReport(Report::new(ServerError::InvalidParameter {
            message: "test".to_string(),
        }));
        assert_eq!(invalid_param.status(), StatusCode::BAD_REQUEST);

        let not_found = ServerErrorReport(Report::new(ServerError::NotFound {
            resource: "test".to_string(),
        }));
        assert_eq!(not_found.status(), StatusCode::NOT_FOUND);

        let internal = ServerErrorReport(Report::new(ServerError::StateStore));
        assert_eq!(internal.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let not_impl = ServerErrorReport(Report::new(ServerError::NotImplemented {
            feature: "test".to_string(),
        }));
        assert_eq!(not_impl.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn test_helper_functions() {
        let err = not_found("test resource");
        assert_eq!(err.status(), StatusCode::NOT_FOUND);

        let err = invalid_parameter("test message");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);

        let err = invalid_uuid("invalid-uuid");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);

        let err = not_implemented("test feature");
        assert_eq!(err.status(), StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn test_state_error_conversion() {
        let state_err = Report::new(StateError::BlobNotFound {
            blob_id: "test-blob".to_string(),
        });
        let server_err: ServerErrorReport = state_err.into();
        assert_eq!(server_err.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_execution_error_conversion() {
        let exec_err = Report::new(ExecutionError::BlobNotFound {
            blob_id: "test-blob".to_string(),
        });
        let server_err: ServerErrorReport = exec_err.into();
        assert_eq!(server_err.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_plugin_error_conversion() {
        let plugin_err = Report::new(PluginError::Execution);
        let server_err: ServerErrorReport = plugin_err.into();
        assert_eq!(server_err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
