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

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::{ServiceExt, Service};
use serde_json::json;
use std::sync::Arc;
use stepflow_core::values::ValueTemplate;
use stepflow_core::{
    FlowResult,
    workflow::{Flow, FlowBuilder, StepBuilder},
};
use stepflow_execution::StepflowExecutor;
use stepflow_mock::MockPlugin;
use stepflow_observability::{
    BinaryObservabilityConfig, LogDestinationType, LogFormat, ObservabilityConfig,
    init_observability,
};
use stepflow_plugin::DynPlugin;
use stepflow_state::InMemoryStateStore;
use tower::ServiceExt as _;

static INIT_TEST_LOGGING: std::sync::Once = std::sync::Once::new();

/// Makes sure logging is initialized for test.
pub fn init_test_logging() {
    INIT_TEST_LOGGING.call_once(|| {
        let config = ObservabilityConfig {
            log_level: log::LevelFilter::Trace,
            other_log_level: None,
            log_destination: LogDestinationType::Stdout,
            log_format: LogFormat::Text,
            log_file: None,
            trace_enabled: false,
            otlp_endpoint: None,
            metrics_enabled: false,
        };

        let binary_config = BinaryObservabilityConfig {
            service_name: "stepflow-server-tests",
            include_run_diagnostic: true,
        };

        let guard =
            init_observability(&config, binary_config).expect("Failed to initialize observability");
        // For tests, we'll just leak the guard to avoid the panic on drop
        // In tests, we don't care about flushing telemetry at the end
        guard.leak();
    });
}

/// Helper to create a test server with in-memory state and optional mock plugins
async fn create_test_server(include_mocks: bool) -> (Router, Arc<StepflowExecutor>) {
    use stepflow_core::FlowError;
    use stepflow_mock::MockComponentBehavior;

    let state_store = Arc::new(InMemoryStateStore::new());

    // Build the plugin router
    let mut plugin_router_builder = stepflow_plugin::routing::PluginRouter::builder();

    // Always register builtin plugins for basic functionality
    plugin_router_builder = plugin_router_builder.register_plugin(
        "builtin".to_string(),
        DynPlugin::boxed(stepflow_builtins::Builtins::new()),
    );

    // Optionally register mock plugins for status testing
    if include_mocks {
        let mut mock_plugin = MockPlugin::new();

        // Configure one_output component (without /mock/ prefix)
        mock_plugin
            .mock_component("/one_output")
            .behavior(
                json!({"input": "first_step"}),
                MockComponentBehavior::result(json!({"output": "step1_result"})),
            )
            .behavior(
                json!({"input": "debug_step"}),
                MockComponentBehavior::result(json!({"output": "debug_result"})),
            );

        // Configure two_outputs component (without /mock/ prefix)
        mock_plugin
            .mock_component("/two_outputs")
            .behavior(
                json!({"input": "step1_result"}),
                MockComponentBehavior::result(json!({"x": 42, "y": 100})),
            )
            .behavior(
                json!({"input": "debug_result"}),
                MockComponentBehavior::result(json!({"x": 99, "y": 200})),
            );

        // Configure error_component component (without /mock/ prefix)
        mock_plugin.mock_component("/error_component").behavior(
            json!({"input": "trigger_error"}),
            MockComponentBehavior::result(FlowResult::Failed(FlowError::new(
                500,
                "Mock error for testing",
            ))),
        );

        plugin_router_builder = plugin_router_builder
            .register_plugin("mock".to_string(), DynPlugin::boxed(mock_plugin));
    }

    // Set up routing configuration
    use std::collections::HashMap;
    use stepflow_plugin::routing::{RouteRule, RoutingConfig};

    let mut routes = HashMap::new();

    // Add builtin routes
    routes.insert(
        "/builtin/{*component}".to_string(),
        vec![RouteRule {
            conditions: vec![],
            component_allow: None,
            component_deny: None,
            plugin: "builtin".into(),
            component: None,
        }],
    );

    if include_mocks {
        routes.insert(
            "/mock/{*component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "mock".into(),
                component: None,
            }],
        );
    }

    let routing_config = RoutingConfig { routes };
    plugin_router_builder = plugin_router_builder.with_routing_config(routing_config);

    let plugin_router = plugin_router_builder.build().unwrap();
    let executor = StepflowExecutor::new(state_store, std::path::PathBuf::from("."), plugin_router);
    executor.initialize_plugins().await.unwrap();

    // Use the real startup logic but without swagger UI for tests
    use stepflow_server::AppConfig;
    let config = AppConfig {
        include_swagger: false, // Skip swagger for tests to keep them fast
        include_cors: true,     // Keep CORS for test compatibility
    };

    let app = config.create_app_router(executor.clone(), 7837);

    (app, executor)
}

/// Helper to create a test server with only builtin components
async fn create_basic_test_server() -> (Router, Arc<StepflowExecutor>) {
    create_test_server(false).await
}

/// Helper to create a test server with both builtin and mock components
async fn create_test_server_with_mocks() -> (Router, Arc<StepflowExecutor>) {
    create_test_server(true).await
}

/// Helper to create a simple test workflow
fn create_test_workflow() -> Flow {
    FlowBuilder::test_flow()
        .description("Test workflow for integration testing")
        .step(
            StepBuilder::builtin_step("test_step", "create_messages")
                .input_literal(json!({
                    "user_prompt": "Hello from test"
                }))
                .build(),
        )
        .build()
}

/// Helper to create a workflow with multiple steps for override testing
fn create_multi_step_workflow() -> Flow {
    FlowBuilder::test_flow()
        .description("Multi-step workflow for override testing")
        .step(
            StepBuilder::builtin_step("step1", "create_messages")
                .input_literal(json!({
                    "user_prompt": "Original prompt",
                    "temperature": 0.5
                }))
                .build(),
        )
        .step(
            StepBuilder::builtin_step("step2", "eval")
                .input_literal(json!({
                    "expression": "42"
                }))
                .build(),
        )
        .build()
}

#[tokio::test]
async fn test_create_run_without_overrides() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    // First, store the flow to get a flow ID
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

    let store_response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(store_request)
        .await
        .unwrap();
    assert_eq!(store_response.status(), StatusCode::OK);

    let store_body = to_bytes(store_response.into_body(), usize::MAX).await.unwrap();
    let store_result: serde_json::Value = serde_json::from_slice(&store_body).unwrap();
    let flow_id = store_result["flowId"].as_str().unwrap();

    // Execute run without overrides
    let create_run_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": {
                    "test_input": "Hello from run execution"
                },
                "debug": false
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(create_run_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(execute_response["runId"].is_string());
    assert_eq!(execute_response["status"], "completed");
    assert!(!execute_response["debug"].as_bool().unwrap());
    assert!(execute_response["result"].is_object());
}

#[tokio::test]
async fn test_create_run_with_overrides() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_multi_step_workflow();

    // First, store the flow to get a flow ID
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

    let store_response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(store_request)
        .await
        .unwrap();
    assert_eq!(store_response.status(), StatusCode::OK);

    let store_body = to_bytes(store_response.into_body(), usize::MAX).await.unwrap();
    let store_result: serde_json::Value = serde_json::from_slice(&store_body).unwrap();
    let flow_id = store_result["flowId"].as_str().unwrap();

    // Execute run with overrides
    let create_run_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": {
                    "test_input": "Hello with overrides"
                },
                "overrides": {
                    "step1": {
                        "value": {
                            "input": {
                                "temperature": 0.8,
                                "user_prompt": "Overridden prompt"
                            }
                        }
                    },
                    "step2": {
                        "$type": "merge_patch",
                        "value": {
                            "input": {
                                "expression": "100"
                            }
                        }
                    }
                },
                "debug": false
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(create_run_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(execute_response["runId"].is_string());
    assert_eq!(execute_response["status"], "completed");
    assert!(execute_response["result"].is_object());

    // The overrides should have been applied during execution
    // We can't easily verify the exact changes without inspecting internal state,
    // but successful execution indicates the overrides were processed correctly
}

#[tokio::test]
async fn test_create_run_with_invalid_overrides() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    // First, store the flow to get a flow ID
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

    let store_response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(store_request)
        .await
        .unwrap();
    assert_eq!(store_response.status(), StatusCode::OK);

    let store_body = to_bytes(store_response.into_body(), usize::MAX).await.unwrap();
    let store_result: serde_json::Value = serde_json::from_slice(&store_body).unwrap();
    let flow_id = store_result["flowId"].as_str().unwrap();

    // Execute run with overrides that reference a non-existent step
    let create_run_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": {
                    "test_input": "Hello"
                },
                "overrides": {
                    "nonexistent_step": {
                        "value": {
                            "input": {
                                "temperature": 0.8
                            }
                        }
                    }
                },
                "debug": false
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(create_run_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(
        error_response["message"]
            .as_str()
            .unwrap()
            .contains("Failed to apply overrides")
    );
}

#[tokio::test]
async fn test_create_run_empty_overrides() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    // First, store the flow to get a flow ID
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

    let store_response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(store_request)
        .await
        .unwrap();
    assert_eq!(store_response.status(), StatusCode::OK);

    let store_body = to_bytes(store_response.into_body(), usize::MAX).await.unwrap();
    let store_result: serde_json::Value = serde_json::from_slice(&store_body).unwrap();
    let flow_id = store_result["flowId"].as_str().unwrap();

    // Execute run with empty overrides object
    let create_run_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": {
                    "test_input": "Hello"
                },
                "overrides": {},
                "debug": false
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(create_run_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(execute_response["runId"].is_string());
    assert_eq!(execute_response["status"], "completed");
    assert!(execute_response["result"].is_object());
}

#[tokio::test]
async fn test_health_endpoint() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;

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

    let (app, _executor) = create_basic_test_server().await;
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

    let flow_id = store_response["flowId"].as_str().unwrap();
    assert!(!flow_id.is_empty());

    // Get flow
    let get_request = Request::builder()
        .uri(format!("/api/v1/flows/{flow_id}"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(get_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(get_response["flowId"], flow_id);
    assert_eq!(get_response["flow"]["name"], "test_workflow");

    // Check that analysis is included in the get_flow response
    assert!(get_response["analysis"].is_object());
    assert_eq!(get_response["analysis"]["flowId"], flow_id);
    assert!(get_response["analysis"]["steps"].is_object());
}

#[tokio::test]
async fn test_hash_based_execution() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;
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
    let flow_id = store_response["flowId"].as_str().unwrap();

    // Execute flow using hash (non-debug mode)
    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
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

    let (app, _executor) = create_basic_test_server().await;
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
    let flow_id = store_response["flowId"].as_str().unwrap();

    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
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
    assert_eq!(details_response["flowId"], flow_id);
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

    assert_eq!(flow_response["flowId"], flow_id);
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

    assert!(steps_response["steps"].is_object());
    assert!(!steps_response["steps"].as_object().unwrap().is_empty());
}

#[tokio::test]
async fn test_list_runs() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;

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

    let (app, _executor) = create_basic_test_server().await;

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

    let (app, _executor) = create_basic_test_server().await;

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

    let (app, _executor) = create_basic_test_server().await;

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

#[tokio::test]
async fn test_status_updates_during_regular_execution() {
    init_test_logging();

    let (app, _executor) = create_test_server_with_mocks().await;

    // Create workflow for regular execution status testing
    let workflow = FlowBuilder::new()
        .name("status_test_workflow")
        .description("Test workflow for status tracking")
        .steps(vec![
            StepBuilder::new("step1")
                .component("/mock/one_output")
                .input_json(json!({"input": "first_step"}))
                .build(),
            StepBuilder::new("step2")
                .component("/mock/two_outputs")
                .input_json(json!({
                    "input": {
                        "$from": {"step": "step1"},
                        "path": "output"
                    }
                }))
                .build(),
        ])
        .output(
            ValueTemplate::parse_value(json!({
                "step1_result": {"$from": {"step": "step1"}, "path": "output"},
                "step2_result": {"$from": {"step": "step2"}, "path": "x"}
            }))
            .unwrap(),
        )
        .build();

    // Store the workflow
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
    let flow_id = store_response["flowId"].as_str().unwrap();

    // Execute workflow in regular mode
    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": {},
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

    // Verify execution completed successfully
    assert_eq!(execute_response["status"], "completed");
    assert_eq!(execute_response["result"]["outcome"], "success");

    // Check step statuses after completion
    let steps_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/steps"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(steps_request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let steps_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let steps = steps_response["steps"].as_object().unwrap();
    assert_eq!(steps.len(), 2);

    // Verify step structure and status information - now using dictionary access
    let step1 = &steps["step1"];
    let step2 = &steps["step2"];

    assert_eq!(step1["stepId"], "step1");
    assert_eq!(step1["component"], "/mock/one_output");
    assert_eq!(step2["stepId"], "step2");
    assert_eq!(step2["component"], "/mock/two_outputs");

    // With the standardized field naming, verify steps have a status field
    assert!(step1.get("status").is_some());
    assert!(step2.get("status").is_some());
}

#[tokio::test]
async fn test_status_updates_during_debug_execution() {
    init_test_logging();

    let (app, _executor) = create_test_server_with_mocks().await;

    // Create workflow for debug execution status testing
    let workflow = FlowBuilder::new()
        .name("debug_status_test")
        .description("Test workflow for debug status tracking")
        .steps(vec![
            StepBuilder::new("step1")
                .component("/mock/one_output")
                .input_json(json!({"input": "debug_step"}))
                .build(),
            StepBuilder::new("step2")
                .component("/mock/two_outputs")
                .input_json(json!({
                    "input": {
                        "$from": {"step": "step1"},
                        "path": "output"
                    }
                }))
                .build(),
        ])
        .output(
            ValueTemplate::parse_value(json!({
                "result": {"$from": {"step": "step2"}, "path": "x"}
            }))
            .unwrap(),
        )
        .build();

    // Store the workflow
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
    let flow_id = store_response["flowId"].as_str().unwrap();

    // Execute workflow in debug mode
    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": {},
                "debug": true
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

    // Verify execution is in debug mode and paused
    assert_eq!(execute_response["debug"], true);
    assert_eq!(execute_response["status"], "paused");

    // In debug mode, execution should be paused and awaiting step-by-step control
    // Let's verify the initial step statuses
    let steps_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/steps"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(steps_request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let steps_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let steps = steps_response["steps"].as_object().unwrap();
    assert_eq!(steps.len(), 2);

    // Now using dictionary access for much cleaner test code
    let step1 = &steps["step1"];
    let step2 = &steps["step2"];

    // Verify that status field is used consistently
    assert_eq!(step1["stepId"], "step1");
    assert_eq!(step1["component"], "/mock/one_output");
    assert_eq!(step2["stepId"], "step2");
    assert_eq!(step2["component"], "/mock/two_outputs");

    // Verify that steps have status information
    assert!(step1.get("status").is_some());
    assert!(step2.get("status").is_some());

    // Test that debug API endpoint exists and is accessible
    let runnable_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/debug/runnable"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(runnable_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The key test for debug mode is that:
    // 1. The workflow starts with debug=true and status="paused"
    // 2. Debug API endpoints are accessible for step-by-step control
    // 3. Step status information is properly tracked with the standardized field names
}

#[tokio::test]
async fn test_status_transitions_with_error_handling() {
    init_test_logging();

    let (app, _executor) = create_test_server_with_mocks().await;

    // Create workflow for error status testing
    let workflow = FlowBuilder::new()
        .name("error_status_test")
        .description("Test workflow for error status tracking")
        .step(
            StepBuilder::new("failing_step")
                .component("/mock/error_component")
                .input_literal(json!({"input": "trigger_error"}))
                .build(),
        )
        .output(
            ValueTemplate::parse_value(json!({
                "result": {"$from": {"step": "failing_step"}, "path": "output"}
            }))
            .unwrap(),
        )
        .build();

    // Store the workflow
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
    let flow_id = store_response["flowId"].as_str().unwrap();

    // Execute workflow (should fail)
    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": {},
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

    // Verify execution failed
    assert_eq!(execute_response["status"], "failed");
    assert_eq!(execute_response["result"]["outcome"], "failed");

    // Check step status shows failure
    let steps_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/steps"))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(steps_request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let steps_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let steps = steps_response["steps"].as_object().unwrap();
    assert_eq!(steps.len(), 1);

    // Much cleaner access with dictionary API
    let failing_step = &steps["failing_step"];
    assert_eq!(failing_step["stepId"], "failing_step");
    assert_eq!(failing_step["component"], "/mock/error_component");

    // The key test for error handling is that the workflow overall failed (verified above)
    // and that step information is accessible via the improved dictionary API
}
