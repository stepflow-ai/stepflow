use poem_openapi::ApiResponse;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stepflow_core::status::ExecutionStatus;
use stepflow_core::{FlowResult, workflow::FlowRef};

use super::api_type::ApiType;

/// Error response for API operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ErrorResponse {
    /// Error code
    pub code: u16,
    /// Error message
    pub message: String,
    /// Additional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// API wrapper for ErrorResponse
pub type ApiErrorResponse = ApiType<ErrorResponse>;

/// Response containing a workflow definition and its hash
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowResponse {
    /// The workflow definition
    pub workflow: FlowRef,
    /// The workflow hash
    pub workflow_hash: String,
}

/// API wrapper for WorkflowResponse
pub type ApiWorkflowResponse = ApiType<WorkflowResponse>;

/// Response for workflow execution operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteResponse {
    /// The execution ID (UUID string)
    pub execution_id: String,
    /// The result of the execution (if completed synchronously)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
    /// Current status of the execution
    pub status: ExecutionStatus,
    /// Whether this execution is in debug mode
    #[serde(default)]
    pub debug: bool,
}

/// API wrapper for ExecuteResponse
pub type ApiExecuteResponse = ApiType<ExecuteResponse>;

/// API response types
#[derive(ApiResponse)]
pub enum ApiResult<T: poem_openapi::types::ToJSON + Send + Sync> {
    /// Success response
    #[oai(status = 200)]
    Ok(poem_openapi::payload::Json<T>),
    /// Bad request
    #[oai(status = 400)]
    BadRequest(poem_openapi::payload::Json<ApiErrorResponse>),
    /// Not found
    #[oai(status = 404)]
    NotFound(poem_openapi::payload::Json<ApiErrorResponse>),
    /// Internal server error
    #[oai(status = 500)]
    InternalServerError(poem_openapi::payload::Json<ApiErrorResponse>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use poem_openapi::payload::Json;

    #[test]
    fn test_error_response_creation() {
        let error = ErrorResponse {
            code: 404,
            message: "Not found".to_string(),
            details: Some(serde_json::json!({"resource": "workflow"})),
        };

        assert_eq!(error.code, 404);
        assert_eq!(error.message, "Not found");
        assert!(error.details.is_some());
    }

    #[test]
    fn test_error_response_serialization() {
        let error = ErrorResponse {
            code: 500,
            message: "Internal error".to_string(),
            details: None,
        };

        let json = serde_json::to_string(&error).unwrap();
        let deserialized: ErrorResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.code, 500);
        assert_eq!(deserialized.message, "Internal error");
        assert!(deserialized.details.is_none());
    }

    #[test]
    fn test_api_result_ok() {
        let result: ApiResult<String> = ApiResult::Ok(Json("success".to_string()));

        match result {
            ApiResult::Ok(payload) => {
                assert_eq!(payload.0, "success");
            }
            _ => panic!("Expected Ok variant"),
        }
    }

    #[test]
    fn test_api_result_bad_request() {
        let error = ErrorResponse {
            code: 400,
            message: "Bad request".to_string(),
            details: None,
        };

        let result: ApiResult<String> = ApiResult::BadRequest(Json(ApiType(error)));

        match result {
            ApiResult::BadRequest(payload) => {
                assert_eq!(payload.0.code, 400);
                assert_eq!(payload.0.message, "Bad request");
            }
            _ => panic!("Expected BadRequest variant"),
        }
    }

    #[test]
    fn test_api_result_not_found() {
        let error = ErrorResponse {
            code: 404,
            message: "Resource not found".to_string(),
            details: Some(serde_json::json!({"id": "123"})),
        };

        let result: ApiResult<String> = ApiResult::NotFound(Json(ApiType(error)));

        match result {
            ApiResult::NotFound(payload) => {
                assert_eq!(payload.0.code, 404);
                assert_eq!(payload.0.message, "Resource not found");
                assert!(payload.0.details.is_some());
            }
            _ => panic!("Expected NotFound variant"),
        }
    }

    #[test]
    fn test_api_result_internal_server_error() {
        let error = ErrorResponse {
            code: 500,
            message: "Internal server error".to_string(),
            details: None,
        };

        let result: ApiResult<String> = ApiResult::InternalServerError(Json(ApiType(error)));

        match result {
            ApiResult::InternalServerError(payload) => {
                assert_eq!(payload.0.code, 500);
                assert_eq!(payload.0.message, "Internal server error");
                assert!(payload.0.details.is_none());
            }
            _ => panic!("Expected InternalServerError variant"),
        }
    }
}
