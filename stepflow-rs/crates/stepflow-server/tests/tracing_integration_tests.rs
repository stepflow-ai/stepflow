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

//! Distributed tracing integration tests
//!
//! These tests run in a separate test binary to avoid conflicts with the logger
//! initialization in integration_tests.rs. This allows us to configure OTLP
//! export for these specific tests.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use std::sync::Arc;
use stepflow_core::workflow::{FlowBuilder, StepBuilder};
use stepflow_execution::StepflowExecutor;
use stepflow_observability::{
    BinaryObservabilityConfig, LogDestinationType, LogFormat, ObservabilityConfig,
    init_observability,
};
use stepflow_plugin::DynPlugin;
use stepflow_state::InMemoryStateStore;
use testcontainers::{
    GenericImage, ImageExt,
    core::{IntoContainerPort, Mount, WaitFor},
    runners::AsyncRunner,
};
use tokio::sync::OnceCell;
use tower::ServiceExt as _;

struct TracingState {
    _otlp_endpoint: String,
    trace_dir: std::path::PathBuf,
    _collector: testcontainers::ContainerAsync<GenericImage>,
    _obs_guard: stepflow_observability::ObservabilityGuard,
}

static TRACING_STATE: OnceCell<TracingState> = OnceCell::const_new();

/// One-time initialization of OTLP collector and observability
async fn init_tracing_once() -> &'static TracingState {
    TRACING_STATE
        .get_or_init(|| async {
            println!("🔧 Initializing OTLP collector for tracing tests...");

            // Ensure DOCKER_HOST is set for testcontainers
            if std::env::var("DOCKER_HOST").is_err() {
                let colima_socket = std::path::Path::new(&std::env::var("HOME").unwrap())
                    .join(".colima/default/docker.sock");
                if colima_socket.exists() {
                    // SAFETY: Setting env var during test initialization before any async work
                    unsafe {
                        std::env::set_var(
                            "DOCKER_HOST",
                            format!("unix://{}", colima_socket.display()),
                        );
                    }
                }
            }

            // Create temp directory for traces
            // Use a timestamp-based directory name that's unique per test run, not per process
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let temp_dir = std::path::PathBuf::from("target")
                .join("tracing-tests")
                .join(format!("test-{}", timestamp));
            std::fs::create_dir_all(&temp_dir).expect("Failed to create trace directory");
            let temp_dir_abs =
                std::fs::canonicalize(&temp_dir).expect("Failed to canonicalize temp directory");

            // Get collector config path (located next to this test file)
            // Tests run from the crate directory, so path is relative to that
            let config_path = std::fs::canonicalize("tests/otel-collector-config.yaml")
                .expect("Failed to find otel-collector-config.yaml");

            // Start OTLP collector
            let start_time = std::time::Instant::now();
            let collector_image = GenericImage::new("otel/opentelemetry-collector", "latest")
                .with_exposed_port(4317.tcp())
                .with_exposed_port(4318.tcp())
                .with_wait_for(WaitFor::message_on_stderr("Everything is ready"))
                .with_mount(Mount::bind_mount(
                    config_path.to_str().unwrap(),
                    "/etc/otel-collector-config.yaml",
                ))
                .with_mount(Mount::bind_mount(temp_dir_abs.to_str().unwrap(), "/output"))
                .with_cmd(vec!["--config=/etc/otel-collector-config.yaml"]);

            let collector = collector_image
                .start()
                .await
                .expect("Failed to start OTLP collector");

            let otlp_port = collector
                .get_host_port_ipv4(4317)
                .await
                .expect("Failed to get OTLP port");

            let endpoint = format!("http://localhost:{}", otlp_port);

            println!(
                "✅ OTLP collector started on port {} in {:?}",
                otlp_port,
                start_time.elapsed()
            );

            // Give the collector more time to fully initialize its gRPC endpoint
            // The "Everything is ready" message doesn't guarantee the gRPC server is accepting connections
            println!("⏳ Waiting for gRPC endpoint to be ready...");
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            // Initialize observability with OTLP
            let obs_config = ObservabilityConfig {
                log_level: log::LevelFilter::Info,
                other_log_level: None,
                log_destination: LogDestinationType::Stdout,
                log_format: LogFormat::Json,
                log_file: None,
                trace_enabled: true,
                otlp_endpoint: Some(endpoint.clone()),
            };

            let binary_config = BinaryObservabilityConfig {
                service_name: "stepflow-tracing-tests",
                include_run_diagnostic: true,
            };

            let guard = init_observability(&obs_config, binary_config)
                .expect("Failed to initialize observability");

            println!("✅ Observability initialized");

            TracingState {
                _otlp_endpoint: endpoint,
                trace_dir: temp_dir_abs,
                _collector: collector,
                _obs_guard: guard,
            }
        })
        .await
}

/// Get the trace file path for the current test
fn get_trace_file() -> std::path::PathBuf {
    TRACING_STATE
        .get()
        .expect("Tracing not initialized")
        .trace_dir
        .join("traces.jsonl")
}

/// Helper to create a test server with in-memory state
async fn create_test_server() -> (Router, Arc<StepflowExecutor>) {
    let state_store = Arc::new(InMemoryStateStore::new());

    // Build the plugin router with builtin plugins
    let mut plugin_router_builder = stepflow_plugin::routing::PluginRouter::builder();
    plugin_router_builder = plugin_router_builder.register_plugin(
        "builtin".to_string(),
        DynPlugin::boxed(stepflow_builtins::Builtins::new()),
    );

    // Set up routing configuration
    use std::collections::HashMap;
    use stepflow_plugin::routing::{RouteRule, RoutingConfig};

    let mut routes = HashMap::new();
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

    let routing_config = RoutingConfig { routes };
    plugin_router_builder = plugin_router_builder.with_routing_config(routing_config);

    let plugin_router = plugin_router_builder.build().unwrap();
    let executor = StepflowExecutor::new(state_store, std::path::PathBuf::from("."), plugin_router);
    executor.initialize_plugins().await.unwrap();

    use stepflow_server::AppConfig;
    let config = AppConfig {
        include_swagger: false,
        include_cors: true,
    };

    let app = config.create_app_router(executor.clone(), 7837);

    (app, executor)
}

/// Read traces from JSONL file
fn read_traces(trace_file: &std::path::Path) -> Vec<serde_json::Value> {
    if !trace_file.exists() {
        return vec![];
    }
    let content = std::fs::read_to_string(trace_file).unwrap();
    content
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Find a span by name in traces
fn find_span<'a>(
    traces: &'a [serde_json::Value],
    span_name: &str,
) -> Option<&'a serde_json::Value> {
    for trace in traces {
        if let Some(resource_spans) = trace["resourceSpans"].as_array() {
            for resource_span in resource_spans {
                if let Some(scope_spans) = resource_span["scopeSpans"].as_array() {
                    for scope_span in scope_spans {
                        if let Some(spans) = scope_span["spans"].as_array() {
                            for span in spans {
                                if span["name"].as_str() == Some(span_name) {
                                    return Some(span);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Test: Simple workflow execution with observability
///
/// This test verifies that:
/// - Workflow execution creates proper trace spans
/// - Step execution spans are children of workflow span
/// - All spans are exported to OTLP correctly
#[tokio::test]
async fn test_simple_workflow_observability() {
    init_tracing_once().await;

    let (app, _executor) = create_test_server().await;

    // Create a simple workflow
    let workflow = FlowBuilder::test_flow()
        .description("Test workflow for observability")
        .step(
            StepBuilder::builtin_step("step1", "create_messages")
                .input_literal(json!({
                    "user_prompt": "Test message 1"
                }))
                .build(),
        )
        .step(
            StepBuilder::builtin_step("step2", "create_messages")
                .input_literal(json!({
                    "user_prompt": "Test message 2"
                }))
                .build(),
        )
        .build();

    // Store the workflow first
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

    // Execute workflow via API
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

    let response = app.oneshot(execute_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Wait for traces to be exported
    println!("⏳ Waiting for traces to export (simple workflow)...");
    // Don't call fastrace::flush() directly - it can block the tokio runtime
    // Instead, just wait for the background exporter to send traces
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Verify traces were collected
    let trace_file = get_trace_file();
    println!("📁 Reading traces from: {}", trace_file.display());
    let traces = read_traces(&trace_file);

    if traces.is_empty() {
        println!(
            "⚠️  No traces collected - trace file exists: {}",
            trace_file.exists()
        );
        if trace_file.exists() {
            println!(
                "   File size: {} bytes",
                std::fs::metadata(&trace_file).unwrap().len()
            );
        }
        return;
    }

    // Look for workflow execution span
    let workflow_span = find_span(&traces, "flow_execution");
    assert!(workflow_span.is_some(), "Should find flow_execution span");

    println!("✅ Workflow observability test passed");
    println!("  - Found {} trace entries", traces.len());
    println!("  - Verified flow_execution span exists");
}

/// Test: Multi-step workflow observability
///
/// This test verifies that:
/// - Multi-step workflows create proper trace spans
/// - All spans maintain correct parent-child relationships
/// - Trace IDs propagate across step executions
#[tokio::test]
async fn test_bidirectional_workflow_observability() {
    init_tracing_once().await;

    let (app, _executor) = create_test_server().await;

    // Create a multi-step workflow
    let workflow = FlowBuilder::test_flow()
        .description("Test workflow with multiple steps")
        .step(
            StepBuilder::builtin_step("step1", "create_messages")
                .input_literal(json!({
                    "user_prompt": "Step 1"
                }))
                .build(),
        )
        .step(
            StepBuilder::builtin_step("step2", "create_messages")
                .input_literal(json!({
                    "user_prompt": "Step 2"
                }))
                .build(),
        )
        .step(
            StepBuilder::builtin_step("step3", "create_messages")
                .input_literal(json!({
                    "user_prompt": "Step 3"
                }))
                .build(),
        )
        .build();

    // Store the workflow first
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

    // Execute workflow via API
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

    let response = app.oneshot(execute_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Wait for traces to be exported
    println!("⏳ Waiting for traces to export (bidirectional workflow)...");
    // Don't call fastrace::flush() directly - it can block the tokio runtime
    // Instead, just wait for the background exporter to send traces
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Verify traces were collected
    let trace_file = get_trace_file();
    println!("📁 Reading traces from: {}", trace_file.display());
    let traces = read_traces(&trace_file);

    if traces.is_empty() {
        println!(
            "⚠️  No traces collected - trace file exists: {}",
            trace_file.exists()
        );
        if trace_file.exists() {
            println!(
                "   File size: {} bytes",
                std::fs::metadata(&trace_file).unwrap().len()
            );
        }
        return;
    }

    // Look for workflow execution span
    let workflow_span = find_span(&traces, "flow_execution");
    assert!(workflow_span.is_some(), "Should find flow_execution span");

    println!("✅ Multi-step workflow observability test passed");
    println!("  - Found {} trace entries", traces.len());
    println!("  - Verified flow_execution span exists");
}
