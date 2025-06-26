use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use std::sync::Arc;
use stepflow_core::workflow::{Component, ErrorAction, Flow, Step, ValueRef};
use stepflow_execution::StepFlowExecutor;
use stepflow_plugin::DynPlugin;
use stepflow_state::InMemoryStateStore;
use tower::ServiceExt as _;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use url::Url;

static INIT_TEST_LOGGING: std::sync::Once = std::sync::Once::new();

/// Makes sure logging is initialized for test.
pub fn init_test_logging() {
    INIT_TEST_LOGGING.call_once(|| {
        let fmt_layer = tracing_subscriber::fmt::layer();

        tracing_subscriber::registry()
            .with(EnvFilter::new("stepflow_=trace,info"))
            .with(fmt_layer)
            .with(tracing_error::ErrorLayer::default())
            .try_init()
            .unwrap();
    });
}

/// Helper to create a test server with in-memory state
async fn create_test_server() -> (Router, Arc<StepFlowExecutor>) {
    let state_store = Arc::new(InMemoryStateStore::new());
    let executor = StepFlowExecutor::new(state_store, std::path::PathBuf::from("."));

    executor
        .register_plugin(
            "builtin".to_owned(),
            DynPlugin::boxed(stepflow_builtins::Builtins),
        )
        .await
        .unwrap();

    // Use the real startup logic but without swagger UI for tests
    use stepflow_server::AppConfig;
    let config = AppConfig {
        include_swagger: false, // Skip swagger for tests to keep them fast
        include_cors: true,     // Keep CORS for test compatibility
    };

    let app = config.create_app_router(executor.clone(), 7837);

    (app, executor)
}

/// Helper to create a simple test workflow
fn create_test_workflow() -> Flow {
    Flow {
        name: Some("test_workflow".to_string()),
        description: Some("Test workflow for integration testing".to_string()),
        version: None,
        input_schema: None,
        output_schema: None,
        steps: vec![Step {
            id: "test_step".to_string(),
            component: Component::new(Url::parse("builtin://create_messages").unwrap()),
            input: ValueRef::new(json!({
                "user_prompt": "Hello from test"
            })),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::Fail,
        }],
        output: json!(null).into(),
        test: None,
        examples: vec![],
    }
}

#[tokio::test]
async fn test_health_endpoint() {
    init_test_logging();

    let (app, _executor) = create_test_server().await;

    let request = Request::builder()
        .uri("/api/v1/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let health_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(health_response["status"], "healthy");
    assert!(health_response["timestamp"].is_string());
    assert!(health_response["version"].is_string());
}

#[tokio::test]
async fn test_flow_crud_operations() {
    init_test_logging();

    let (app, _executor) = create_test_server().await;
    let workflow = create_test_workflow();

    // Store flow
    let store_request = Request::builder()
        .uri("/api/v1/flows")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flow": workflow
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = app.clone().oneshot(store_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let store_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let flow_hash = store_response["flowHash"].as_str().unwrap();
    assert!(!flow_hash.is_empty());

    // Get flow
    let get_request = Request::builder()
        .uri(format!("/api/v1/flows/{flow_hash}"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(get_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(get_response["flowHash"], flow_hash);
    assert_eq!(get_response["flow"]["name"], "test_workflow");

    // Check that analysis is included in the get_flow response
    assert!(get_response["analysis"].is_object());
    assert_eq!(get_response["analysis"]["flowHash"], flow_hash);
    assert!(get_response["analysis"]["steps"].is_object());
}

#[tokio::test]
async fn test_hash_based_execution() {
    init_test_logging();

    let (app, _executor) = create_test_server().await;
    let workflow = create_test_workflow();

    // First, store the flow to get a hash
    let store_request = Request::builder()
        .uri("/api/v1/flows")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flow": workflow
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = app.clone().oneshot(store_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let store_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let flow_hash = store_response["flowHash"].as_str().unwrap();

    // Execute flow using hash (non-debug mode)
    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowHash": flow_hash,
                "input": {"message": "test input"},
                "debug": false
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = app.clone().oneshot(execute_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(execute_response["runId"].is_string());
    assert_eq!(execute_response["debug"], false);
    assert_eq!(execute_response["status"], "completed");
    // Workflow completes successfully with null output as defined in workflow
    assert_eq!(execute_response["result"]["outcome"], "success");
    assert!(execute_response["result"]["result"].is_null());
}

#[tokio::test]
async fn test_run_details() {
    init_test_logging();

    let (app, _executor) = create_test_server().await;
    let workflow = create_test_workflow();

    // Store and execute flow
    let store_request = Request::builder()
        .uri("/api/v1/flows")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flow": workflow
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = app.clone().oneshot(store_request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let store_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let flow_hash = store_response["flowHash"].as_str().unwrap();

    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowHash": flow_hash,
                "input": {"message": "test input"},
                "debug": false
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = app.clone().oneshot(execute_request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = execute_response["runId"].as_str().unwrap();

    // Get run details
    let details_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(details_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let details_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(details_response["runId"], run_id);
    assert_eq!(details_response["flowHash"], flow_hash);
    assert_eq!(details_response["status"], "completed");
    assert!(details_response["input"].is_object());

    // Get run flow
    let flow_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/flow"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(flow_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let flow_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(flow_response["flowHash"], flow_hash);
    assert_eq!(flow_response["flow"]["name"], "test_workflow");

    // Get run steps
    let steps_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/steps"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(steps_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let steps_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(steps_response["steps"].is_array());
    assert!(!steps_response["steps"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_list_runs() {
    init_test_logging();

    let (app, _executor) = create_test_server().await;

    // List runs (should be empty initially)
    let list_request = Request::builder()
        .uri("/api/v1/runs")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(list_response["runs"].is_array());
    // Could be empty or have runs from other tests, so just check structure
}

#[tokio::test]
async fn test_components_endpoint() {
    init_test_logging();

    let (app, _executor) = create_test_server().await;

    let request = Request::builder()
        .uri("/api/v1/components")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let components_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(components_response["components"].is_array());
    // Should have at least builtin components
    assert!(
        !components_response["components"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn test_cors_headers() {
    init_test_logging();

    let (app, _executor) = create_test_server().await;

    let request = Request::builder()
        .uri("/api/v1/health")
        .header("Origin", "http://localhost:3000")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    // Check that CORS headers are present
    assert!(
        response
            .headers()
            .contains_key("access-control-allow-origin")
    );
}

#[tokio::test]
async fn test_error_responses() {
    init_test_logging();

    let (app, _executor) = create_test_server().await;

    // Test 404 for non-existent flow
    let request = Request::builder()
        .uri("/api/v1/flows/nonexistent")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Test 404 for non-existent run
    let request = Request::builder()
        .uri("/api/v1/runs/00000000-0000-0000-0000-000000000000")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Test 400 for invalid request
    let request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from("invalid json"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
