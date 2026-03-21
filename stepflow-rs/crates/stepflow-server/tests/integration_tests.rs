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
use stepflow_plugin::{DynPlugin, StepflowEnvironment, initialize_environment};
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
                stepflow_core::TaskErrorCode::ComponentFailed,
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
            params: std::collections::HashMap::new(),
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
                params: std::collections::HashMap::new(),
            }],
        );
    }

    let routing_config = RoutingConfig { routes };
    plugin_router_builder = plugin_router_builder.with_routing_config(routing_config);

    let plugin_router = plugin_router_builder.build().unwrap();

    let env = Arc::new(StepflowEnvironment::new());
    env.insert(metadata_store);
    env.insert(blob_store);
    env.insert(execution_journal);
    env.insert(std::path::PathBuf::from("."));
    env.insert(Arc::new(plugin_router) as Arc<stepflow_plugin::routing::PluginRouter>);
    initialize_environment(&env).await.unwrap();
    let executor = env;

    // Use the real startup logic but without swagger UI for tests
    use stepflow_server::ServiceOptions;
    let options = ServiceOptions {
        include_cors: true,
        ..Default::default()
    };

    let app = options.create_app_router(executor.clone(), 7837);

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

/// Helper: store a flow via proto route and return its flow_id.
async fn store_flow(app: &mut Router, workflow: &Flow) -> String {
    let request = Request::builder()
        .uri("/proto/api/v1/flows")
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

/// Helper: execute a run (wait=true) via proto route and return the run_id.
async fn execute_run(app: &mut Router, flow_id: &str, input: serde_json::Value) -> String {
    let request = Request::builder()
        .uri("/proto/api/v1/runs")
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
    result["summary"]["runId"].as_str().unwrap().to_string()
}

// =============================================================================
// SSE helpers
// =============================================================================

/// Parse tonic-rest SSE text into (event_type, data_json) tuples.
///
/// tonic-rest SSE events have only `data:` fields (no `event:` or `id:` SSE fields).
/// The event type is determined by which oneof key is present in the `event` object
/// within the JSON data (e.g., `RunCreated`, `StepStarted`, `RunCompleted`).
/// prost-wkt serializes oneof variants with PascalCase keys.
fn parse_sse_events(text: &str) -> Vec<(String, serde_json::Value)> {
    let mut events = Vec::new();
    let mut current_data = String::new();

    for line in text.lines() {
        if line.starts_with("data:") {
            if !current_data.is_empty() {
                current_data.push('\n');
            }
            current_data.push_str(line["data:".len()..].trim_start());
        } else if line.is_empty() && !current_data.is_empty() {
            // Event boundary
            let data: serde_json::Value =
                serde_json::from_str(&current_data).unwrap_or(json!(current_data));
            let event_type = determine_event_type(&data);
            events.push((event_type, data));
            current_data.clear();
        }
    }
    // Flush trailing event (stream may close without final blank line)
    if !current_data.is_empty() {
        let data: serde_json::Value =
            serde_json::from_str(&current_data).unwrap_or(json!(current_data));
        let event_type = determine_event_type(&data);
        events.push((event_type, data));
    }
    events
}

/// Determine the event type from the oneof key in the `event` field.
/// prost-wkt serializes oneof variants with PascalCase keys.
fn determine_event_type(data: &serde_json::Value) -> String {
    if let Some(event_obj) = data.get("event") {
        let oneof_keys = [
            "RunCreated",
            "StepStarted",
            "StepCompleted",
            "StepReady",
            "ItemCompleted",
            "RunCompleted",
            "SubRunCreated",
            "StepsNeeded",
        ];
        for key in &oneof_keys {
            if event_obj.get(key).is_some() {
                return key.to_string();
            }
        }
    }
    "unknown".to_string()
}

// =============================================================================
// REST Tests via proto routes
// =============================================================================

#[tokio::test]
async fn test_health_endpoint() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;

    let request = Request::builder()
        .uri("/proto/api/v1/health")
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
    // Proto health response has a `build` object (not `version`)
    assert!(health_response["build"].is_object());
}

#[tokio::test]
async fn test_flow_crud_operations() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    // Store flow
    let store_request = Request::builder()
        .uri("/proto/api/v1/flows")
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
    // Proto StoreFlowResponse has camelCase fields
    assert!(store_response["stored"].is_boolean());
    assert!(store_response["diagnostics"].is_object());

    // Get flow
    let get_request = Request::builder()
        .uri(format!("/proto/api/v1/flows/{flow_id}"))
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
async fn test_create_run_without_overrides() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute run without overrides (wait=true for synchronous result)
    let create_run_request = Request::builder()
        .uri("/proto/api/v1/runs")
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

    // Proto CreateRunResponse has nested summary
    assert!(execute_response["summary"]["runId"].is_string());
    // status==2 means Completed
    assert_eq!(execute_response["summary"]["status"], 2);
    assert!(execute_response["summary"]["flowId"].is_string());
    assert!(execute_response["summary"]["items"].is_object());
    assert!(execute_response["summary"]["createdAt"].is_string());
    // Results are populated with wait=true
    assert!(execute_response["results"].is_array());
    assert_eq!(execute_response["results"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_create_run_async_default() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute run without wait (default async behavior)
    let create_run_request = Request::builder()
        .uri("/proto/api/v1/runs")
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

    // Proto routes always return 200 OK (not 202 for async creates)
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(execute_response["summary"]["runId"].is_string());
    // status==1 means Running
    assert_eq!(execute_response["summary"]["status"], 1);
    assert!(execute_response["summary"]["flowId"].is_string());
    // Results should be empty array (not null) for proto repeated fields
    assert_eq!(execute_response["results"], json!([]));
    let run_id = execute_response["summary"]["runId"].as_str().unwrap();

    // Now use GET /proto/api/v1/runs/{run_id}?wait=true to wait for completion
    let wait_request = Request::builder()
        .uri(format!("/proto/api/v1/runs/{run_id}?wait=true"))
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

    // GetRunResponse has nested summary + steps array
    assert_eq!(details_response["summary"]["runId"], run_id);
    // status==2 means Completed
    assert_eq!(details_response["summary"]["status"], 2);
}

#[tokio::test]
async fn test_create_run_with_overrides() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_multi_step_workflow();

    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute run with overrides (wait=true for synchronous result)
    let create_run_request = Request::builder()
        .uri("/proto/api/v1/runs")
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

    assert!(execute_response["summary"]["runId"].is_string());
    // status==2 means Completed
    assert_eq!(execute_response["summary"]["status"], 2);
    assert!(execute_response["results"].is_array());
}

#[tokio::test]
async fn test_create_run_with_invalid_overrides() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute run with overrides that reference a non-existent step
    let create_run_request = Request::builder()
        .uri("/proto/api/v1/runs")
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
    // Overrides are validated upfront - referencing non-existent steps should fail.
    // gRPC INVALID_ARGUMENT maps to HTTP 400.
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 Bad Request for invalid overrides, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_create_run_empty_overrides() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute run with empty overrides object (wait=true for synchronous result)
    let create_run_request = Request::builder()
        .uri("/proto/api/v1/runs")
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

    assert!(execute_response["summary"]["runId"].is_string());
    // status==2 means Completed
    assert_eq!(execute_response["summary"]["status"], 2);
    assert!(execute_response["results"].is_array());
}

#[tokio::test]
async fn test_hash_based_execution() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute flow using hash (wait=true for synchronous result)
    let execute_request = Request::builder()
        .uri("/proto/api/v1/runs")
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

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(execute_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(execute_response["summary"]["runId"].is_string());
    // status==2 means Completed
    assert_eq!(execute_response["summary"]["status"], 2);
    // Results populated with wait=true
    let results = execute_response["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    // Proto ItemResult has status (integer) and output fields, not result.outcome
    assert_eq!(results[0]["status"], 2); // Completed
}

#[tokio::test]
async fn test_run_details() {
    init_test_logging();

    let (mut app, _executor) = create_basic_test_server().await;
    let workflow = create_test_workflow();

    let flow_id = store_flow(&mut app, &workflow).await;
    let run_id = execute_run(&mut app, &flow_id, json!({"message": "test input"})).await;

    // Get run details
    let details_request = Request::builder()
        .uri(format!("/proto/api/v1/runs/{run_id}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(details_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let details_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // GetRunResponse has nested summary + steps array
    assert_eq!(details_response["summary"]["runId"], run_id);
    assert_eq!(details_response["summary"]["flowId"], flow_id);
    // status==2 means Completed
    assert_eq!(details_response["summary"]["status"], 2);
    // Proto GetRunResponse has steps as an array
    assert!(details_response["steps"].is_array());

    // Get run steps
    let steps_request = Request::builder()
        .uri(format!("/proto/api/v1/runs/{run_id}/steps"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(steps_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let steps_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // GetRunStepsResponse has { "steps": [...] } — an array of StepStatus objects
    assert!(steps_response["steps"].is_array());
}

#[tokio::test]
async fn test_list_runs() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;

    // List runs (should be empty initially)
    let list_request = Request::builder()
        .uri("/proto/api/v1/runs")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(list_response["runs"].is_array());
}

#[tokio::test]
async fn test_components_endpoint() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;

    let request = Request::builder()
        .uri("/proto/api/v1/components")
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
        .uri("/proto/api/v1/health")
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

    // Test error for non-existent flow.
    // "nonexistent" is not a valid blob ID format, so the proto route returns
    // 400 (INVALID_ARGUMENT) rather than 404.
    let request = Request::builder()
        .uri("/proto/api/v1/flows/nonexistent")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 BAD_REQUEST for invalid flow_id, got {}",
        response.status()
    );

    // Test 404 for non-existent run (valid UUID format but doesn't exist)
    let request = Request::builder()
        .uri("/proto/api/v1/runs/00000000-0000-0000-0000-000000000000")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Test 400 for invalid request
    let request = Request::builder()
        .uri("/proto/api/v1/runs")
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

    let (mut app, _executor) = create_test_server_with_mocks().await;

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

    // Store and execute
    let flow_id = store_flow(&mut app, &workflow).await;

    let execute_request = Request::builder()
        .uri("/proto/api/v1/runs")
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

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(execute_request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = execute_response["summary"]["runId"].as_str().unwrap();

    // Verify execution completed successfully (status==2)
    assert_eq!(execute_response["summary"]["status"], 2);
    let results = execute_response["results"].as_array().unwrap();
    // Proto ItemResult has status field (integer)
    assert_eq!(results[0]["status"], 2); // Completed

    // Check step statuses after completion
    let steps_request = Request::builder()
        .uri(format!("/proto/api/v1/runs/{run_id}/steps"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(steps_request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let steps_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Proto GetRunStepsResponse has steps as an array
    let steps = steps_response["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);

    // Find steps by stepId in the array
    let step1 = steps.iter().find(|s| s["stepId"] == "step1").unwrap();
    let step2 = steps.iter().find(|s| s["stepId"] == "step2").unwrap();

    assert_eq!(step1["stepId"], "step1");
    assert_eq!(step2["stepId"], "step2");

    // Verify steps have a status field (integer enum)
    assert!(step1.get("status").is_some());
    assert!(step2.get("status").is_some());
}

#[tokio::test]
async fn test_status_transitions_with_error_handling() {
    init_test_logging();

    let (mut app, _executor) = create_test_server_with_mocks().await;

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

    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute workflow (should fail; wait=true for synchronous result)
    let execute_request = Request::builder()
        .uri("/proto/api/v1/runs")
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

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(execute_request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = execute_response["summary"]["runId"].as_str().unwrap();

    // Verify execution failed (status==3)
    assert_eq!(execute_response["summary"]["status"], 3);
    let results = execute_response["results"].as_array().unwrap();
    // Proto ItemResult has status field (integer) - 3 means Failed
    assert_eq!(results[0]["status"], 3);

    // Check step status shows failure
    let steps_request = Request::builder()
        .uri(format!("/proto/api/v1/runs/{run_id}/steps"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(steps_request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let steps_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Proto GetRunStepsResponse has steps as an array
    let steps = steps_response["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1);

    let failing_step = &steps[0];
    assert_eq!(failing_step["stepId"], "failing_step");
}

#[tokio::test]
async fn test_blob_json_round_trip() {
    init_test_logging();

    let (app, _executor) = create_basic_test_server().await;

    let test_data = json!({"message": "Hello, blobs!", "number": 42, "nested": {"key": "value"}});

    // Store a JSON blob via proto route
    let store_request = Request::builder()
        .uri("/proto/api/v1/blobs")
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
        .uri(format!("/proto/api/v1/blobs/{blob_id}"))
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

    // Store a binary blob via proto route
    let store_request = Request::builder()
        .uri("/proto/api/v1/blobs")
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
        .uri(format!("/proto/api/v1/blobs/{blob_id}"))
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
//
// tonic-rest SSE events have only `data:` fields (no `event:` or `id:` SSE fields).
// The event type is in the `event` oneof within the JSON data, using PascalCase
// keys (e.g., `RunCreated`, `StepStarted`, `RunCompleted`).
//
// The SSE handler uses Query-only extraction (no Path), so `runId` must be passed
// as a query parameter.
// =============================================================================

#[tokio::test]
async fn test_sse_stream_basic() {
    init_test_logging();

    let (mut app, _env) = create_basic_test_server().await;
    let workflow = create_test_workflow();
    let flow_id = store_flow(&mut app, &workflow).await;

    // Execute the run to completion first (wait=true)
    let run_id = execute_run(&mut app, &flow_id, json!({"test_input": "hello"})).await;

    // Now stream events for the completed run — should replay all events and close.
    // SSE handler requires runId in query string (no path extraction).
    let sse_request = Request::builder()
        .uri(format!("/proto/api/v1/runs/{run_id}/events?runId={run_id}"))
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

    // Should have at minimum: RunCreated, RunCompleted
    assert!(
        events.len() >= 2,
        "Expected at least 2 SSE events, got {}: {:?}",
        events.len(),
        events.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
    );

    // First event should be RunCreated
    assert_eq!(events[0].0, "RunCreated");

    // Last event should be RunCompleted
    let last = events.last().unwrap();
    assert_eq!(last.0, "RunCompleted");
    // status==2 means Completed
    assert_eq!(last.1["event"]["RunCompleted"]["status"], 2);

    // All events should have sequenceNumber and timestamp in the data payload
    for (event_type, data) in &events {
        assert!(
            data.get("sequenceNumber").is_some(),
            "Event {event_type} missing sequenceNumber in data"
        );
        assert!(
            data.get("timestamp").is_some(),
            "Event {event_type} missing timestamp in data"
        );
    }

    // Sequence numbers should be monotonically non-decreasing
    let seq_numbers: Vec<u64> = events
        .iter()
        .map(|(_, data)| data["sequenceNumber"].as_u64().unwrap())
        .collect();
    for window in seq_numbers.windows(2) {
        assert!(
            window[0] <= window[1],
            "Sequence numbers not monotonic: {} > {}",
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
        .uri(format!("/proto/api/v1/runs/{run_id}/events?runId={run_id}"))
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
    assert!(all_events.len() >= 2);

    // Get the sequence number of the first event and resume from after it
    let first_seq = all_events[0].1["sequenceNumber"].as_u64().unwrap();
    let resume_from = first_seq + 1;

    let sse_request = Request::builder()
        .uri(format!(
            "/proto/api/v1/runs/{run_id}/events?runId={run_id}&since={resume_from}"
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
    let resumed_events = parse_sse_events(&String::from_utf8(body.to_vec()).unwrap());

    // Should have fewer events (skipped the first ones)
    assert!(
        resumed_events.len() < all_events.len(),
        "Resumed stream should have fewer events: {} vs {}",
        resumed_events.len(),
        all_events.len()
    );

    // All resumed events should have sequenceNumber >= resume_from
    for (_, data) in &resumed_events {
        let seq = data["sequenceNumber"].as_u64().unwrap();
        assert!(
            seq >= resume_from,
            "Resumed event has sequenceNumber {} < since {}",
            seq,
            resume_from
        );
    }

    // Should still end with RunCompleted
    assert_eq!(resumed_events.last().unwrap().0, "RunCompleted");
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

    // Stream with includeResults=true
    let sse_request = Request::builder()
        .uri(format!(
            "/proto/api/v1/runs/{run_id}/events?runId={run_id}&includeResults=true"
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

    // Find StepCompleted events — they should have result field in the oneof data
    let step_completed: Vec<_> = events
        .iter()
        .filter(|(t, _)| t == "StepCompleted")
        .collect();
    assert!(
        !step_completed.is_empty(),
        "Should have StepCompleted events"
    );
    for (_, data) in &step_completed {
        assert!(
            data["event"]["StepCompleted"].get("result").is_some(),
            "StepCompleted should include result when includeResults=true"
        );
    }

    // Stream without includeResults (default)
    let sse_request = Request::builder()
        .uri(format!("/proto/api/v1/runs/{run_id}/events?runId={run_id}"))
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
        .filter(|(t, _)| t == "StepCompleted")
        .collect();
    for (_, data) in &step_completed {
        assert!(
            data["event"]["StepCompleted"].get("result").is_none(),
            "StepCompleted should NOT include result by default"
        );
    }
}

#[tokio::test]
async fn test_sse_stream_not_found() {
    init_test_logging();

    let (mut app, _env) = create_basic_test_server().await;

    let fake_run_id = uuid::Uuid::now_v7();
    let sse_request = Request::builder()
        .uri(format!(
            "/proto/api/v1/runs/{fake_run_id}/events?runId={fake_run_id}"
        ))
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
        .uri(format!("/proto/api/v1/runs/{run_id}/events?runId={run_id}"))
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

    let event_types: Vec<&str> = events.iter().map(|(t, _)| t.as_str()).collect();

    // Should have expected lifecycle events (PascalCase oneof keys)
    assert!(
        event_types.contains(&"RunCreated"),
        "Missing RunCreated: {event_types:?}"
    );
    assert!(
        event_types.contains(&"StepStarted"),
        "Missing StepStarted: {event_types:?}"
    );
    assert!(
        event_types.contains(&"StepCompleted"),
        "Missing StepCompleted: {event_types:?}"
    );
    assert!(
        event_types.contains(&"RunCompleted"),
        "Missing RunCompleted: {event_types:?}"
    );

    // Should have step events for both steps
    let step_started_count = event_types.iter().filter(|&&t| t == "StepStarted").count();
    assert!(
        step_started_count >= 2,
        "Expected at least 2 StepStarted events, got {}",
        step_started_count
    );

    let step_completed_count = event_types
        .iter()
        .filter(|&&t| t == "StepCompleted")
        .count();
    assert!(
        step_completed_count >= 2,
        "Expected at least 2 StepCompleted events, got {}",
        step_completed_count
    );

    // Step events should include stepId (resolved from the flow)
    let step_started_events: Vec<_> = events.iter().filter(|(t, _)| t == "StepStarted").collect();
    for (_, data) in &step_started_events {
        assert!(
            data["event"]["StepStarted"].get("stepId").is_some(),
            "StepStarted should include stepId, got: {data}"
        );
    }
    let step_ids: Vec<&str> = step_started_events
        .iter()
        .map(|(_, d)| d["event"]["StepStarted"]["stepId"].as_str().unwrap())
        .collect();
    assert!(
        step_ids.contains(&"step1"),
        "Should have step1: {step_ids:?}"
    );
    assert!(
        step_ids.contains(&"step2"),
        "Should have step2: {step_ids:?}"
    );
}

// =============================================================================
// gRPC Service Tests (call trait methods directly)
// =============================================================================

#[tokio::test]
async fn test_grpc_step_detail_asof_consistent() {
    init_test_logging();

    let (mut app, env) = create_basic_test_server().await;
    let workflow = FlowBuilder::test_flow()
        .step(
            StepBuilder::builtin_step("test_step", "create_messages")
                .input_literal(json!({"user_prompt": "hello"}))
                .build(),
        )
        .output(ValueExpr::step_output("test_step"))
        .build();
    let flow_id = store_flow(&mut app, &workflow).await;
    let run_id = execute_run(&mut app, &flow_id, json!({"test_input": "grpc_test"})).await;

    // Call GetStepDetail directly via the gRPC service trait
    use stepflow_grpc::RunsServiceImpl;
    use stepflow_grpc::proto::stepflow::v1::GetStepDetailRequest;
    use stepflow_grpc::proto::stepflow::v1::runs_service_server::RunsService;

    let service = RunsServiceImpl::new(env.clone());

    // Use asof=0 which should always be consistent (step journal_seqno >= 0)
    let response = service
        .get_step_detail(tonic::Request::new(GetStepDetailRequest {
            run_id: run_id.to_string(),
            step_id: "test_step".to_string(),
            item_index: Some(0),
            asof: Some(0),
        }))
        .await;

    assert!(
        response.is_ok(),
        "GetStepDetail with asof<=journal_seqno should succeed, got: {:?}",
        response.err()
    );
    let resp = response.unwrap().into_inner();
    assert!(resp.step.is_some());
    assert_eq!(resp.step.unwrap().step_id, "test_step");
}

#[tokio::test]
async fn test_grpc_step_detail_asof_inconsistent() {
    init_test_logging();

    let (mut app, env) = create_basic_test_server().await;
    let workflow = FlowBuilder::test_flow()
        .step(
            StepBuilder::builtin_step("test_step", "create_messages")
                .input_literal(json!({"user_prompt": "hello"}))
                .build(),
        )
        .output(ValueExpr::step_output("test_step"))
        .build();
    let flow_id = store_flow(&mut app, &workflow).await;
    let run_id = execute_run(&mut app, &flow_id, json!({"test_input": "grpc_test"})).await;

    // Call GetStepDetail with a very high asof that exceeds the journal seqno
    use stepflow_grpc::RunsServiceImpl;
    use stepflow_grpc::proto::stepflow::v1::GetStepDetailRequest;
    use stepflow_grpc::proto::stepflow::v1::runs_service_server::RunsService;

    let service = RunsServiceImpl::new(env.clone());

    let response = service
        .get_step_detail(tonic::Request::new(GetStepDetailRequest {
            run_id: run_id.to_string(),
            step_id: "test_step".to_string(),
            item_index: Some(0),
            asof: Some(999999),
        }))
        .await;

    assert!(
        response.is_err(),
        "GetStepDetail with asof>journal_seqno should return FAILED_PRECONDITION"
    );
    let status = response.unwrap_err();
    assert_eq!(
        status.code(),
        tonic::Code::FailedPrecondition,
        "Expected FAILED_PRECONDITION, got {:?}",
        status.code()
    );
}
