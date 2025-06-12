//! Tests for the StepFlow HTTP server shared modules
//!
//! These tests focus on shared data structures and common functionality.
//! Individual endpoint tests are located in their respective modules.

use serde_json::json;
use std::sync::Arc;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::{Component, ErrorAction, Flow, FlowRef, Step, ValueRef};
use stepflow_execution::StepFlowExecutor;
use stepflow_state::InMemoryStateStore;
use url::Url;

use super::{
    api_type::ApiType,
    common::{ApiResult, ErrorResponse, ExecuteResponse},
};

/// Helper to create a test executor for testing
async fn create_test_executor() -> Arc<StepFlowExecutor> {
    let state_store = Arc::new(InMemoryStateStore::new());
    StepFlowExecutor::new(state_store)
}

/// Helper to create a simple test workflow
fn create_test_workflow() -> Flow {
    Flow {
        name: Some("test_workflow".to_string()),
        description: Some("Test workflow for API testing".to_string()),
        version: None,
        input_schema: None,
        output_schema: None,
        steps: vec![Step {
            id: "test_step".to_string(),
            component: Component::new(Url::parse("builtin://messages").unwrap()),
            input: ValueRef::new(json!("Hello from test")),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::Fail,
        }],
        output: serde_json::Value::Null,
        test: None,
    }
}

/// Helper to create test input data
fn create_test_input() -> ValueRef {
    ValueRef::new(json!({
        "message": "test input"
    }))
}

#[test]
fn test_execute_response_structure() {
    let response = ExecuteResponse {
        execution_id: "test-uuid".to_string(),
        result: None,
        status: ExecutionStatus::Running,
        debug: false,
    };

    assert_eq!(response.execution_id, "test-uuid");
    assert!(response.result.is_none());
    assert!(matches!(response.status, ExecutionStatus::Running));
}

#[test]
fn test_error_response_creation() {
    let error = ErrorResponse {
        code: 404,
        message: "Not found".to_string(),
        details: Some(json!({"resource": "workflow"})),
    };

    assert_eq!(error.code, 404);
    assert_eq!(error.message, "Not found");
    assert!(error.details.is_some());
}

#[test]
fn test_api_result_variants() {
    use poem_openapi::payload::Json;

    // Test Ok variant
    let ok_result: ApiResult<String> = ApiResult::Ok(Json("success".to_string()));
    assert!(matches!(ok_result, ApiResult::Ok(_)));

    // Test error variants
    let error = ErrorResponse {
        code: 400,
        message: "Bad request".to_string(),
        details: None,
    };

    let bad_request: ApiResult<String> = ApiResult::BadRequest(Json(ApiType(error)));
    assert!(matches!(bad_request, ApiResult::BadRequest(_)));
}

#[test]
fn test_flow_ref_creation() {
    let workflow = create_test_workflow();
    let flow_ref = FlowRef::new(workflow);

    // Just verify that FlowRef was created successfully
    // FlowRef is a wrapper around Arc<Flow>, so check the inner flow
    assert_eq!(flow_ref.name, Some("test_workflow".to_string()));
}

#[test]
fn test_value_ref_creation() {
    let input = create_test_input();

    // Should be able to extract the value
    let json_value = input.as_ref();
    assert!(json_value["message"].is_string());
    assert_eq!(json_value["message"], "test input");
}

#[tokio::test]
async fn test_executor_creation() {
    let executor = create_test_executor().await;

    // Should be able to get state store
    let state_store = executor.state_store();
    assert!(state_store.list_endpoints(None).await.is_ok());
}

// Test that helper functions work correctly
#[test]
fn test_helper_functions() {
    let workflow = create_test_workflow();
    assert_eq!(workflow.name, Some("test_workflow".to_string()));
    assert!(!workflow.steps.is_empty());

    let input = create_test_input();
    let json_val = input.as_ref();
    assert!(json_val.is_object());
}
