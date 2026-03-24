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
//! cd stepflow-rs && cargo build -p stepflow-cli --no-default-features
//!
//! # Run integration tests from sdks/rust/
//! STEPFLOW_DEV_BINARY=../../stepflow-rs/target/debug/stepflow \
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
        "/builtin/{*component}".to_string(),
        vec![RouteRule {
            plugin: "builtin".to_string(),
        }],
    );
    routes.insert(
        "/test/{*component}".to_string(),
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

#[tokio::test]
#[ignore = "requires STEPFLOW_DEV_BINARY — run with --include-ignored"]
async fn test_double_component() {
    const QUEUE: &str = "test-double";

    // Start local orchestrator — fails with a clear message if STEPFLOW_DEV_BINARY is unset.
    let orch = LocalOrchestrator::start(make_config(QUEUE))
        .await
        .expect("Failed to start local orchestrator");

    // Start in-process worker with /test/double component
    let mut registry = ComponentRegistry::new();
    registry.register_fn("/test/double", |input: DoubleInput, _ctx| async move {
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

    // Give the worker a moment to connect
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Build and submit a flow: /test/double(input) → output
    let mut client = StepflowClient::connect(orch.url())
        .await
        .expect("Failed to connect to orchestrator");

    let mut builder = FlowBuilder::new();
    builder.add_step(
        "/test/double",
        "/test/double",
        stepflow_client::ValueExpr::input(None),
    );
    let flow = builder
        .output(FlowBuilder::step("/test/double"))
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

#[tokio::test]
#[ignore = "requires STEPFLOW_DEV_BINARY — run with --include-ignored"]
async fn test_chained_steps() {
    const QUEUE: &str = "test-chained";

    let orch = LocalOrchestrator::start(make_config(QUEUE))
        .await
        .expect("Failed to start local orchestrator");

    let mut registry = ComponentRegistry::new();

    registry.register_fn("/test/greet", |input: GreetInput, _ctx| async move {
        Ok(GreetOutput {
            message: format!("Hello, {}!", input.name),
        })
    });

    registry.register_fn("/test/shout", |input: ShoutInput, _ctx| async move {
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

    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut client = StepflowClient::connect(orch.url())
        .await
        .expect("Failed to connect");

    // Flow: greet → shout → output.loud
    let mut builder = FlowBuilder::new();
    builder.add_step(
        "greet",
        "/test/greet",
        stepflow_client::ValueExpr::object([("name", FlowBuilder::input().field("name").into())]),
    );
    builder.add_step(
        "shout",
        "/test/shout",
        stepflow_client::ValueExpr::object([(
            "message",
            FlowBuilder::step("greet").field("message").into(),
        )]),
    );
    let flow = builder
        .output(FlowBuilder::step("shout").field("loud"))
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
