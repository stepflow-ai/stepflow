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

//! Integration tests for the Rust SDK.
//!
//! These tests are marked `#[ignore]` and require the `STEPFLOW_DEV_BINARY`
//! environment variable to point to the `stepflow` binary.
//!
//! Tests that are run without the variable set **fail** — they do not silently
//! pass.  Use `#[ignore]` to opt out of running them in environments that do not
//! have the binary available.
//!
//! # Running
//!
//! ```bash
//! # Build the orchestrator binary first (from repo root / stepflow-rs/)
//! cd stepflow-rs && cargo build -p stepflow-server --no-default-features
//!
//! # Run integration tests from sdks/rust/
//! STEPFLOW_DEV_BINARY=../../stepflow-rs/target/debug/stepflow-server \
//!   cargo test --test integration -- --include-ignored
//! ```

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use stepflow_client::local_server::{
    GrpcPluginConfig, LocalOrchestrator, OrchestratorConfig, PluginConfig, RouteRule,
};
use stepflow_client::{FlowBuilder, StepflowClient};
use stepflow_worker::{ComponentRegistry, Worker, WorkerConfig};

/// Initialize tracing once for the test process.
///
/// Calling `try_init` is idempotent — subsequent calls from other tests
/// are silently ignored.  Set `RUST_LOG=debug` (or any valid filter) to
/// control verbosity.
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("stepflow_worker=debug".parse().unwrap()),
        )
        .with_test_writer()
        .try_init();
}

// ---------------------------------------------------------------------------
// Helper: wait until an expected component ID appears in the orchestrator
// ---------------------------------------------------------------------------

/// Poll `list_components` until `component_id` appears, or panic on timeout.
///
/// This replaces arbitrary `sleep` calls and makes tests deterministic under CI load.
async fn wait_for_component(client: &mut StepflowClient, component_id: &str, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("Timed out after {timeout:?} waiting for component '{component_id}' to appear");
        }
        match client.list_components(true).await {
            Ok(result)
                if result
                    .components
                    .iter()
                    .any(|c| c.component == component_id) =>
            {
                return;
            }
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ---------------------------------------------------------------------------
// Helper: build a standard orchestrator config with builtin + rust-worker
// ---------------------------------------------------------------------------

fn make_config(queue_name: &str) -> OrchestratorConfig {
    let mut plugins = HashMap::new();
    plugins.insert("builtin".to_string(), PluginConfig::Builtin);
    plugins.insert(
        "rust-worker".to_string(),
        PluginConfig::Grpc(GrpcPluginConfig {
            queue_name: queue_name.to_string(),
            command: None,
            args: vec![],
            env: HashMap::new(),
        }),
    );

    let mut routes = HashMap::new();
    routes.insert(
        "/builtin".to_string(),
        vec![RouteRule {
            plugin: "builtin".to_string(),
        }],
    );
    routes.insert(
        "/test".to_string(),
        vec![RouteRule {
            plugin: "rust-worker".to_string(),
        }],
    );

    OrchestratorConfig {
        plugins,
        routes,
        storage_config: None,
    }
}

// ---------------------------------------------------------------------------
// Test: double a number using an in-process Rust worker
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DoubleInput {
    value: i64,
}

#[derive(Serialize)]
struct DoubleOutput {
    result: i64,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires STEPFLOW_DEV_BINARY — run with --include-ignored"]
async fn test_double_component() {
    init_tracing();
    const QUEUE: &str = "test-double";

    // Start local orchestrator — fails with a clear message if STEPFLOW_DEV_BINARY is unset.
    let orch = LocalOrchestrator::start(make_config(QUEUE))
        .await
        .expect("Failed to start local orchestrator");

    // Register as "double" — the orchestrator resolves the full path /test/double
    // to component ID "double" and dispatches it to the worker.
    let mut registry = ComponentRegistry::new();
    registry.register_fn("double", |input: DoubleInput, _ctx| async move {
        Ok(DoubleOutput {
            result: input.value * 2,
        })
    });

    let worker_config = WorkerConfig {
        tasks_url: orch.tasks_url().to_string(),
        queue_name: QUEUE.to_string(),
        max_concurrent: 4,
        max_retries: 3,
        shutdown_grace_secs: 5,
        ..Default::default()
    };

    // Run the worker in the background; stop it via a oneshot channel
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let worker_handle = tokio::spawn(async move {
        Worker::new(registry, worker_config)
            .run_until(async move {
                let _ = stop_rx.await;
            })
            .await
    });

    // Wait until the worker has connected and registered its components.
    // list_registered_components returns component IDs (without route prefix),
    // so we poll for "double" (the component ID), not "/test/double".
    let mut client = StepflowClient::connect(orch.url())
        .await
        .expect("Failed to connect to orchestrator");
    wait_for_component(&mut client, "double", Duration::from_secs(60)).await;

    let mut builder = FlowBuilder::new();
    builder.add_step(
        "/test/double",
        "/test/double",
        stepflow_client::ValueExpr::workflow_input(Default::default()),
    );
    let flow = builder
        .output(stepflow_client::ValueExpr::step_output("/test/double"))
        .build()
        .expect("Failed to build flow");

    let flow_id = client
        .store_flow(&flow)
        .await
        .expect("Failed to store flow");

    let result = client
        .run(&flow_id, serde_json::json!({"value": 21}))
        .await
        .expect("Failed to run flow");

    assert_eq!(result, serde_json::json!({"result": 42}));

    // Stop worker
    let _ = stop_tx.send(());
    let _ = worker_handle.await;
}

// ---------------------------------------------------------------------------
// Test: chained steps — greet then shout
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GreetInput {
    name: String,
}

#[derive(Serialize)]
struct GreetOutput {
    message: String,
}

#[derive(Deserialize)]
struct ShoutInput {
    message: String,
}

#[derive(Serialize)]
struct ShoutOutput {
    loud: String,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires STEPFLOW_DEV_BINARY — run with --include-ignored"]
async fn test_chained_steps() {
    init_tracing();
    const QUEUE: &str = "test-chained";

    let orch = LocalOrchestrator::start(make_config(QUEUE))
        .await
        .expect("Failed to start local orchestrator");

    let mut registry = ComponentRegistry::new();

    // Register by component ID — the orchestrator resolves /test/greet to ID "greet".
    registry.register_fn("greet", |input: GreetInput, _ctx| async move {
        Ok(GreetOutput {
            message: format!("Hello, {}!", input.name),
        })
    });

    registry.register_fn("shout", |input: ShoutInput, _ctx| async move {
        Ok(ShoutOutput {
            loud: input.message.to_uppercase(),
        })
    });

    let worker_config = WorkerConfig {
        tasks_url: orch.tasks_url().to_string(),
        queue_name: QUEUE.to_string(),
        max_retries: 3,
        shutdown_grace_secs: 5,
        ..Default::default()
    };

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let worker_handle = tokio::spawn(async move {
        Worker::new(registry, worker_config)
            .run_until(async move {
                let _ = stop_rx.await;
            })
            .await
    });

    let mut client = StepflowClient::connect(orch.url())
        .await
        .expect("Failed to connect");
    // list_registered_components returns raw component IDs (without route prefix).
    wait_for_component(&mut client, "greet", Duration::from_secs(60)).await;

    // Flow: greet → shout → output.loud
    let mut builder = FlowBuilder::new();
    builder.add_step(
        "greet",
        "/test/greet",
        stepflow_client::ValueExpr::object(vec![(
            "name".to_string(),
            stepflow_client::ValueExpr::workflow_input(
                stepflow_flow::values::JsonPath::parse("$.name").unwrap(),
            ),
        )]),
    );
    builder.add_step(
        "shout",
        "/test/shout",
        stepflow_client::ValueExpr::object(vec![(
            "message".to_string(),
            stepflow_client::ValueExpr::step(
                "greet",
                stepflow_flow::values::JsonPath::parse("$.message").unwrap(),
            ),
        )]),
    );
    let flow = builder
        .output(stepflow_client::ValueExpr::step(
            "shout",
            stepflow_flow::values::JsonPath::parse("$.loud").unwrap(),
        ))
        .build()
        .expect("Failed to build flow");

    let flow_id = client
        .store_flow(&flow)
        .await
        .expect("Failed to store flow");

    let result = client
        .run(&flow_id, serde_json::json!({"name": "world"}))
        .await
        .expect("Failed to run flow");

    assert_eq!(result, serde_json::json!("HELLO, WORLD!"));

    let _ = stop_tx.send(());
    let _ = worker_handle.await;
}

// ---------------------------------------------------------------------------
// Test: auto-blobification of large outputs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct BlobInput {
    data: String,
}

#[derive(Serialize)]
struct BlobOutput {
    echo: String,
    small: String,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires STEPFLOW_DEV_BINARY — run with --include-ignored"]
async fn test_auto_blobification() {
    init_tracing();
    const QUEUE: &str = "test-blob";

    let orch = LocalOrchestrator::start(make_config(QUEUE))
        .await
        .expect("Failed to start local orchestrator");

    let mut registry = ComponentRegistry::new();

    // Component echoes its input data back, plus a small field.
    // With a low threshold, the "echo" field should be blobified
    // in the response, while "small" stays inline.
    registry.register_fn("blob_echo", |input: BlobInput, _ctx| async move {
        Ok(BlobOutput {
            echo: input.data,
            small: "tiny".to_string(),
        })
    });

    // Set a low blob threshold (256 bytes) so the large field triggers blobification
    let worker_config = WorkerConfig {
        tasks_url: orch.tasks_url().to_string(),
        queue_name: QUEUE.to_string(),
        max_retries: 3,
        shutdown_grace_secs: 5,
        blob_threshold_bytes: 256,
        ..Default::default()
    };

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let worker_handle = tokio::spawn(async move {
        Worker::new(registry, worker_config)
            .run_until(async move {
                let _ = stop_rx.await;
            })
            .await
    });

    let mut client = StepflowClient::connect(orch.url())
        .await
        .expect("Failed to connect");
    wait_for_component(&mut client, "blob_echo", Duration::from_secs(60)).await;

    // Build a flow that sends a large input and returns the output
    let mut builder = FlowBuilder::new();
    builder.add_step(
        "blob_echo",
        "/test/blob_echo",
        stepflow_client::ValueExpr::workflow_input(Default::default()),
    );
    let flow = builder
        .output(stepflow_client::ValueExpr::step_output("blob_echo"))
        .build()
        .expect("Failed to build flow");

    let flow_id = client
        .store_flow(&flow)
        .await
        .expect("Failed to store flow");

    // Create a large input string (> 256 bytes threshold)
    let large_data = "x".repeat(1024);
    let result = client
        .run(&flow_id, serde_json::json!({"data": large_data}))
        .await
        .expect("Failed to run flow");

    // The worker blobifies the large "echo" field and returns a blob ref.
    // The orchestrator passes blob refs through to the client as-is.
    let result_obj = result.as_object().expect("Expected object result");

    // Large field should be a blob ref (object with $blob key)
    let echo_val = result_obj.get("echo").expect("Expected 'echo' field");
    let blob_ref = echo_val
        .as_object()
        .expect("Expected 'echo' to be a blob ref object");
    let blob_id = blob_ref
        .get("$blob")
        .and_then(|v| v.as_str())
        .expect("Expected '$blob' key with string value");
    assert_eq!(blob_id.len(), 64, "Blob ID should be a 64-char SHA-256 hex");
    assert_eq!(
        blob_ref.get("blobType").and_then(|v| v.as_str()),
        Some("data"),
        "Blob ref should have blobType 'data'"
    );
    assert!(
        blob_ref.get("size").and_then(|v| v.as_u64()).unwrap_or(0) > 256,
        "Blob ref should record the original size"
    );

    // Small field should pass through inline (not blobified)
    assert_eq!(
        result_obj.get("small").and_then(|v| v.as_str()),
        Some("tiny"),
        "Small field should pass through unchanged"
    );

    let _ = stop_tx.send(());
    let _ = worker_handle.await;
}
