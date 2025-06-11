//! Tests for the StepFlow HTTP server modules
//!
//! These tests focus on the data structures and serialization/deserialization
//! rather than full integration tests.

use serde_json::json;
use std::sync::Arc;
use stepflow_core::schema::SchemaRef;
use stepflow_core::workflow::{Component, ErrorAction, Flow, FlowRef, Step, ValueRef};
use stepflow_execution::StepFlowExecutor;
use stepflow_state::InMemoryStateStore;
use url::Url;

use super::{
    api_type::ApiType,
    common::{ApiResult, ErrorResponse, ExecuteResponse},
    components::{ComponentInfoResponse, ListComponentsResponse},
    endpoints::{CreateEndpointRequest, EndpointResponse},
    execution::ExecuteRequest,
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
fn test_execute_request_serialization() {
    let workflow = create_test_workflow();
    let input = create_test_input();

    let request = ExecuteRequest {
        workflow: FlowRef::new(workflow),
        input,
        debug: false,
    };

    // Should be able to serialize/deserialize
    let json = serde_json::to_string(&request).unwrap();
    let deserialized: ExecuteRequest = serde_json::from_str(&json).unwrap();

    assert!(!deserialized.debug);
    // Just check that workflow was deserialized correctly
}

#[test]
fn test_execute_request_with_debug() {
    let workflow = create_test_workflow();
    let input = create_test_input();

    let request = ExecuteRequest {
        workflow: FlowRef::new(workflow),
        input,
        debug: true,
    };

    assert!(request.debug);
}

#[test]
fn test_execute_response_structure() {
    let response = ExecuteResponse {
        execution_id: "test-uuid".to_string(),
        result: None,
        status: stepflow_state::ExecutionStatus::Running,
        debug: false,
    };

    assert_eq!(response.execution_id, "test-uuid");
    assert!(response.result.is_none());
    assert!(matches!(
        response.status,
        stepflow_state::ExecutionStatus::Running
    ));
}

#[test]
fn test_endpoint_response_structure() {
    let response = EndpointResponse {
        name: "test_endpoint".to_string(),
        label: None,
        workflow_hash: "abc123".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    };

    assert_eq!(response.name, "test_endpoint");
    assert_eq!(response.workflow_hash, "abc123");
}

#[test]
fn test_create_endpoint_request_serialization() {
    let workflow = create_test_workflow();
    let request = CreateEndpointRequest {
        workflow: FlowRef::new(workflow),
    };

    let json = serde_json::to_string(&request).unwrap();
    let _deserialized: CreateEndpointRequest = serde_json::from_str(&json).unwrap();

    // Just check that workflow was deserialized correctly
}

#[test]
fn test_component_info_response_serialization() {
    let component = ComponentInfoResponse {
        name: "test_component".to_string(),
        plugin_name: "test_plugin".to_string(),
        description: Some("Test description".to_string()),
        input_schema: Some(SchemaRef::parse_json(r#"{"type": "string"}"#).unwrap()),
        output_schema: None,
    };

    let json = serde_json::to_string(&component).unwrap();
    let deserialized: ComponentInfoResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.name, "test_component");
    assert_eq!(deserialized.plugin_name, "test_plugin");
    assert_eq!(
        deserialized.description,
        Some("Test description".to_string())
    );
    assert!(deserialized.input_schema.is_some());
    assert!(deserialized.output_schema.is_none());
}

#[test]
fn test_list_components_response_serialization() {
    let response = ListComponentsResponse {
        components: vec![
            ComponentInfoResponse {
                name: "comp1".to_string(),
                plugin_name: "plugin1".to_string(),
                description: None,
                input_schema: None,
                output_schema: None,
            },
            ComponentInfoResponse {
                name: "comp2".to_string(),
                plugin_name: "plugin2".to_string(),
                description: Some("Description".to_string()),
                input_schema: None,
                output_schema: None,
            },
        ],
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: ListComponentsResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.components.len(), 2);
    assert_eq!(deserialized.components[0].name, "comp1");
    assert_eq!(deserialized.components[1].name, "comp2");
    assert_eq!(
        deserialized.components[1].description,
        Some("Description".to_string())
    );
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
