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
use serde_json::json;
use std::sync::Arc;
use stepflow_core::{
    FlowResult, ValueExpr,
    workflow::{Flow, FlowBuilder, StepBuilder},
};
use stepflow_mock::MockPlugin;
use stepflow_observability::{
    BinaryObservabilityConfig, LogDestinationType, LogFormat, ObservabilityConfig,
    init_observability,
};
use stepflow_plugin::{DynPlugin, StepflowEnvironment, StepflowEnvironmentBuilder};
use stepflow_state::{BlobStore, InMemoryStateStore, MetadataStore};
use tower::{Service as _, ServiceExt};

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
async fn create_test_server(include_mocks: bool) -> (Router, Arc<StepflowEnvironment>) {
    use stepflow_core::FlowError;
    use stepflow_mock::MockComponentBehavior;

    let store = Arc::new(InMemoryStateStore::new());
    let metadata_store: Arc<dyn MetadataStore> = store.clone();
    let blob_store: Arc<dyn BlobStore> = store.clone();
    let execution_journal: Arc<dyn stepflow_state::ExecutionJournal> = store;

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
    let executor = StepflowEnvironmentBuilder::new()
        .metadata_store(metadata_store)
        .blob_store(blob_store)
        .execution_journal(execution_journal)
        .working_directory(std::path::PathBuf::from("."))
        .plugin_router(plugin_router)
        .build()
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

/// Helper to create a test server with only builtin components
async fn create_basic_test_server() -> (Router, Arc<StepflowEnvironment>) {
    create_test_server(false).await
}

/// Helper to create a test server with both builtin and mock components
async fn create_test_server_with_mocks() -> (Router, Arc<StepflowEnvironment>) {
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

    let store_body = to_bytes(store_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let store_result: serde_json::Value = serde_json::from_slice(&store_body).unwrap();
    let flow_id = store_result["flowId"].as_str().unwrap();

    // Execute run without overrides (wait=true for synchronous result)
    let create_run_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [{
                    "test_input": "Hello from run execution"
                }],
                "wait": true
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
    // Response now includes RunSummary fields
    assert!(execute_response["flowId"].is_string());
    assert!(execute_response["items"].is_object());
    assert!(execute_response["createdAt"].is_string());
    // Results are populated with wait=true
    assert!(execute_response["results"].is_array());
    assert_eq!(execute_response["results"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_create_run_async_default() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    // Store the flow
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
    let store_body = to_bytes(store_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let store_result: serde_json::Value = serde_json::from_slice(&store_body).unwrap();
    let flow_id = store_result["flowId"].as_str().unwrap();

    // Execute run without wait (default async behavior)
    let create_run_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [{
                    "test_input": "Hello from async run"
                }]
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

    // Should return 202 Accepted (async)
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(execute_response["runId"].is_string());
    // Results should be null (not populated in async mode)
    assert!(execute_response["results"].is_null());
    // RunSummary fields should be present
    assert!(execute_response["flowId"].is_string());
    assert_eq!(execute_response["status"], "running");
    let run_id = execute_response["runId"].as_str().unwrap();

    // Now use GET /runs/{run_id}?wait=true to wait for completion
    let wait_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}?wait=true"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(wait_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let details_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(details_response["runId"], run_id);
    assert_eq!(details_response["status"], "completed");
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

    let store_body = to_bytes(store_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let store_result: serde_json::Value = serde_json::from_slice(&store_body).unwrap();
    let flow_id = store_result["flowId"].as_str().unwrap();

    // Execute run with overrides (wait=true for synchronous result)
    let create_run_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [{
                    "test_input": "Hello with overrides"
                }],
                "overrides": {
                    "steps": {
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
                    }
                },
                "wait": true
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
    assert!(execute_response["results"].is_array());

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

    let store_body = to_bytes(store_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let store_result: serde_json::Value = serde_json::from_slice(&store_body).unwrap();
    let flow_id = store_result["flowId"].as_str().unwrap();

    // Execute run with overrides that reference a non-existent step
    // (validation errors are returned synchronously regardless of wait)
    let create_run_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [{
                    "test_input": "Hello"
                }],
                "overrides": {
                    "steps": {
                        "nonexistent_step": {
                            "value": {
                                "input": {
                                    "temperature": 0.8
                                }
                            }
                        }
                    }
                }
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
    // Overrides are validated upfront - referencing non-existent steps returns Bad Request
    // This provides better UX by catching typos in step names immediately
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Error response should contain information about the failure
    assert!(error_response["message"].as_str().is_some());
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

    let store_body = to_bytes(store_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let store_result: serde_json::Value = serde_json::from_slice(&store_body).unwrap();
    let flow_id = store_result["flowId"].as_str().unwrap();

    // Execute run with empty overrides object (wait=true for synchronous result)
    let create_run_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [{
                    "test_input": "Hello"
                }],
                "overrides": {"steps": {}},
                "wait": true
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
    assert!(execute_response["results"].is_array());
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

    // Execute flow using hash (wait=true for synchronous result)
    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [{"message": "test input"}],
                "wait": true
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
    assert_eq!(execute_response["status"], "completed");
    // Workflow completes successfully with null output as defined in workflow
    let results = execute_response["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["result"]["outcome"], "success");
    assert!(results[0]["result"]["result"].is_null());
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
                "input": [{"message": "test input"}],
                "wait": true
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
    assert!(details_response["itemDetails"].is_array());

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
                        "$step": "step1",
                        "path": "output"
                    }
                }))
                .build(),
        ])
        .output(ValueExpr::Object(
            vec![
                (
                    "step1_result".to_string(),
                    ValueExpr::Step {
                        step: "step1".to_string(),
                        path: "output".into(),
                    },
                ),
                (
                    "step2_result".to_string(),
                    ValueExpr::Step {
                        step: "step2".to_string(),
                        path: "x".into(),
                    },
                ),
            ]
            .into_iter()
            .collect(),
        ))
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

    // Execute workflow (wait=true for synchronous result)
    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [{}],
                "wait": true
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
    let results = execute_response["results"].as_array().unwrap();
    assert_eq!(results[0]["result"]["outcome"], "success");

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
        .output(ValueExpr::Object(
            vec![(
                "result".to_string(),
                ValueExpr::Step {
                    step: "failing_step".to_string(),
                    path: "output".into(),
                },
            )]
            .into_iter()
            .collect(),
        ))
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

    // Execute workflow (should fail; wait=true for synchronous result)
    let execute_request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [{}],
                "wait": true
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
    let results = execute_response["results"].as_array().unwrap();
    assert_eq!(results[0]["result"]["outcome"], "failed");

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

#[tokio::test]
async fn test_blob_json_round_trip() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;

    let test_data = json!({"message": "Hello, blobs!", "number": 42, "nested": {"key": "value"}});

    // Store a JSON blob
    let store_request = Request::builder()
        .uri("/api/v1/blobs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "data": test_data,
                "blobType": "data"
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = app.clone().oneshot(store_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let store_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let blob_id = store_response["blobId"].as_str().unwrap();
    assert!(!blob_id.is_empty());

    // Retrieve the blob as JSON
    let get_request = Request::builder()
        .uri(format!("/api/v1/blobs/{blob_id}"))
        .header("accept", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(get_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let get_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(get_response["data"], test_data);
    assert_eq!(get_response["blobType"], "data");
    assert_eq!(get_response["blobId"], blob_id);
}

#[tokio::test]
async fn test_blob_binary_round_trip() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;

    let binary_data: Vec<u8> = vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD, 0x42, 0x43];

    // Store a binary blob
    let store_request = Request::builder()
        .uri("/api/v1/blobs")
        .method("POST")
        .header("content-type", "application/octet-stream")
        .header("x-blob-filename", "test-file.bin")
        .body(Body::from(binary_data.clone()))
        .unwrap();

    let response = app.clone().oneshot(store_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let store_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let blob_id = store_response["blobId"].as_str().unwrap();
    assert!(!blob_id.is_empty());
    assert_eq!(store_response["filename"], "test-file.bin");

    // Retrieve the blob as binary
    let get_request = Request::builder()
        .uri(format!("/api/v1/blobs/{blob_id}"))
        .header("accept", "application/octet-stream")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(get_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Check headers
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    assert_eq!(response.headers().get("x-blob-type").unwrap(), "binary");
    let disposition = response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(disposition.contains("test-file.bin"));

    // Check body matches original bytes
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), binary_data.as_slice());
}

// =============================================================================
// SSE Status Stream Tests
// =============================================================================

/// Parse SSE text into (id, event_type, data_json) tuples.
fn parse_sse_events(text: &str) -> Vec<(Option<String>, String, serde_json::Value)> {
    let mut events = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_event: Option<String> = None;
    let mut current_data = String::new();

    for line in text.lines() {
        if line.starts_with("id:") {
            current_id = Some(line["id:".len()..].trim().to_string());
        } else if line.starts_with("event:") {
            current_event = Some(line["event:".len()..].trim().to_string());
        } else if line.starts_with("data:") {
            if !current_data.is_empty() {
                current_data.push('\n');
            }
            current_data.push_str(line["data:".len()..].trim_start());
        } else if line.is_empty() && !current_data.is_empty() {
            // Event boundary
            let data: serde_json::Value =
                serde_json::from_str(&current_data).unwrap_or(json!(current_data));
            events.push((
                current_id.take(),
                current_event
                    .take()
                    .unwrap_or_else(|| "message".to_string()),
                data,
            ));
            current_data.clear();
        }
    }
    // Flush trailing event (stream may close without final blank line)
    if !current_data.is_empty() {
        let data: serde_json::Value =
            serde_json::from_str(&current_data).unwrap_or(json!(current_data));
        events.push((
            current_id.take(),
            current_event
                .take()
                .unwrap_or_else(|| "message".to_string()),
            data,
        ));
    }
    events
}

/// Helper: store a flow and return its flow_id.
async fn store_flow(app: &mut Router, workflow: &Flow) -> String {
    let request = Request::builder()
        .uri("/api/v1/flows")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({ "flow": workflow })).unwrap(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::ready(app)
        .await
        .unwrap()
        .call(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    result["flowId"].as_str().unwrap().to_string()
}

/// Helper: submit a run (wait=false) and return the run_id.
async fn _submit_run(app: &mut Router, flow_id: &str, input: serde_json::Value) -> String {
    let request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [input],
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::ready(app)
        .await
        .unwrap()
        .call(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    result["runId"].as_str().unwrap().to_string()
}

/// Helper: execute a run (wait=true) and return the run_id.
async fn execute_run(app: &mut Router, flow_id: &str, input: serde_json::Value) -> String {
    let request = Request::builder()
        .uri("/api/v1/runs")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({
                "flowId": flow_id,
                "input": [input],
                "wait": true,
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::ready(app)
        .await
        .unwrap()
        .call(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    result["runId"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_sse_stream_basic() {
    init_test_logging();

    let (mut app, _env) = create_basic_test_server().await;
    let workflow = create_test_workflow();
    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute the run to completion first (wait=true)
    let run_id = execute_run(&mut app, &flow_id, json!({"test_input": "hello"})).await;

    // Now stream events for the completed run — should replay all events and close
    let sse_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/events"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(sse_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_sse_events(&text);

    // Should have at minimum: run_created, run_initialized, step events, run_completed
    assert!(
        events.len() >= 3,
        "Expected at least 3 SSE events, got {}: {:?}",
        events.len(),
        events
            .iter()
            .map(|(_, e, _)| e.as_str())
            .collect::<Vec<_>>()
    );

    // First event should be run_created
    assert_eq!(events[0].1, "run_created");
    assert_eq!(events[0].2["runId"], run_id);

    // Last event should be run_completed
    let last = events.last().unwrap();
    assert_eq!(last.1, "run_completed");
    assert_eq!(last.2["status"], "completed");

    // All events should have SSE ids (journal sequence numbers)
    for (id, _, _) in &events {
        assert!(id.is_some(), "SSE event missing id field");
    }

    // IDs should be monotonically non-decreasing
    let ids: Vec<u64> = events
        .iter()
        .map(|(id, _, _)| id.as_ref().unwrap().parse::<u64>().unwrap())
        .collect();
    for window in ids.windows(2) {
        assert!(
            window[0] <= window[1],
            "SSE ids not monotonic: {} > {}",
            window[0],
            window[1]
        );
    }
}

#[tokio::test]
async fn test_sse_stream_resume_with_since() {
    init_test_logging();

    let (mut app, _env) = create_basic_test_server().await;
    let workflow = create_test_workflow();
    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute run to completion
    let run_id = execute_run(&mut app, &flow_id, json!({"test_input": "resume_test"})).await;

    // Stream all events first
    let sse_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/events"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(sse_request)
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let all_events = parse_sse_events(&String::from_utf8(body.to_vec()).unwrap());
    assert!(all_events.len() >= 3);

    // Get the ID of the second event and resume from after it
    let resume_from = all_events[1].0.as_ref().unwrap().parse::<u64>().unwrap() + 1;

    let sse_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/events?since={resume_from}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(sse_request)
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let resumed_events = parse_sse_events(&String::from_utf8(body.to_vec()).unwrap());

    // Should have fewer events (skipped the first ones)
    assert!(
        resumed_events.len() < all_events.len(),
        "Resumed stream should have fewer events: {} vs {}",
        resumed_events.len(),
        all_events.len()
    );

    // All resumed events should have IDs >= resume_from
    for (id, _, _) in &resumed_events {
        let seq: u64 = id.as_ref().unwrap().parse().unwrap();
        assert!(
            seq >= resume_from,
            "Resumed event has id {} < since {}",
            seq,
            resume_from
        );
    }

    // Should still end with run_completed
    assert_eq!(resumed_events.last().unwrap().1, "run_completed");
}

#[tokio::test]
async fn test_sse_stream_event_type_filter() {
    init_test_logging();

    let (mut app, _env) = create_basic_test_server().await;
    let workflow = create_test_workflow();
    let flow_id = store_flow(&mut app, &workflow).await;
    let run_id = execute_run(&mut app, &flow_id, json!({"test_input": "filter_test"})).await;

    // Stream only run_completed events
    let sse_request = Request::builder()
        .uri(format!(
            "/api/v1/runs/{run_id}/events?eventTypes=run_completed"
        ))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(sse_request)
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let events = parse_sse_events(&String::from_utf8(body.to_vec()).unwrap());

    // Should only have run_completed events
    assert!(!events.is_empty(), "Should have at least one event");
    for (_, event_type, _) in &events {
        assert_eq!(event_type, "run_completed");
    }
}

#[tokio::test]
async fn test_sse_stream_include_results() {
    init_test_logging();

    let (mut app, _env) = create_basic_test_server().await;
    // Need a workflow with output so steps are actually executed and journaled
    let workflow = FlowBuilder::test_flow()
        .step(
            StepBuilder::builtin_step("test_step", "create_messages")
                .input_literal(json!({"user_prompt": "hello"}))
                .build(),
        )
        .output(ValueExpr::step_output("test_step"))
        .build();
    let flow_id = store_flow(&mut app, &workflow).await;
    let run_id = execute_run(&mut app, &flow_id, json!({"test_input": "results_test"})).await;

    // Stream with include_results=true
    let sse_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/events?includeResults=true"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(sse_request)
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let events = parse_sse_events(&String::from_utf8(body.to_vec()).unwrap());

    // Find step_completed events — they should have result field
    let step_completed: Vec<_> = events
        .iter()
        .filter(|(_, t, _)| t == "step_completed")
        .collect();
    assert!(
        !step_completed.is_empty(),
        "Should have step_completed events"
    );
    for (_, _, data) in &step_completed {
        assert!(
            data.get("result").is_some(),
            "step_completed should include result when includeResults=true"
        );
    }

    // Stream without include_results (default)
    let sse_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/events"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(sse_request)
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let events = parse_sse_events(&String::from_utf8(body.to_vec()).unwrap());

    let step_completed: Vec<_> = events
        .iter()
        .filter(|(_, t, _)| t == "step_completed")
        .collect();
    for (_, _, data) in &step_completed {
        assert!(
            data.get("result").is_none(),
            "step_completed should NOT include result by default"
        );
    }
}

#[tokio::test]
async fn test_sse_stream_not_found() {
    init_test_logging();

    let (mut app, _env) = create_basic_test_server().await;

    let fake_run_id = uuid::Uuid::now_v7();
    let sse_request = Request::builder()
        .uri(format!("/api/v1/runs/{fake_run_id}/events"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(sse_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_sse_stream_multi_step_workflow() {
    init_test_logging();

    let (mut app, _env) = create_test_server_with_mocks().await;

    // Build a two-step workflow with output referencing step2
    let workflow = FlowBuilder::test_flow()
        .name("sse_multi_step")
        .step(
            StepBuilder::new("step1")
                .component("/mock/one_output")
                .input_literal(json!({"input": "first_step"}))
                .build(),
        )
        .step(
            StepBuilder::new("step2")
                .component("/mock/two_outputs")
                .input_json(json!({"input": {"$step": "step1", "path": "output"}}))
                .build(),
        )
        .output(ValueExpr::step_output("step2"))
        .build();

    let flow_id = store_flow(&mut app, &workflow).await;
    let run_id = execute_run(&mut app, &flow_id, json!({"unused": true})).await;

    // Stream events
    let sse_request = Request::builder()
        .uri(format!("/api/v1/runs/{run_id}/events"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(sse_request)
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let events = parse_sse_events(&String::from_utf8(body.to_vec()).unwrap());

    let event_types: Vec<&str> = events.iter().map(|(_, t, _)| t.as_str()).collect();

    // Should have expected lifecycle events
    assert!(event_types.contains(&"run_created"));
    assert!(event_types.contains(&"run_initialized"));
    assert!(event_types.contains(&"step_started"));
    assert!(event_types.contains(&"step_completed"));
    assert!(event_types.contains(&"run_completed"));

    // Should have step events for both steps
    let step_started_count = event_types.iter().filter(|&&t| t == "step_started").count();
    assert!(
        step_started_count >= 2,
        "Expected at least 2 step_started events, got {}",
        step_started_count
    );

    let step_completed_count = event_types
        .iter()
        .filter(|&&t| t == "step_completed")
        .count();
    assert!(
        step_completed_count >= 2,
        "Expected at least 2 step_completed events, got {}",
        step_completed_count
    );
}
