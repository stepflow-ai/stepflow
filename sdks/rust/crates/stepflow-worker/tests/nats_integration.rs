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

//! NATS transport integration tests for the Rust SDK.
//!
//! These tests require:
//! 1. `STEPFLOW_DEV_BINARY` — path to the `stepflow-server` binary built with
//!    the `nats` feature.
//! 2. Docker — a NATS server is started via `testcontainers`.
//!
//! # Running
//!
//! ```bash
//! # Build the orchestrator binary with NATS support
//! cd stepflow-rs && cargo build -p stepflow-server --features nats
//!
//! # Run NATS integration tests from sdks/rust/
//! STEPFLOW_DEV_BINARY=../../stepflow-rs/target/debug/stepflow-server \
//!   cargo test --test nats_integration --features nats -- --include-ignored
//! ```

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use testcontainers::core::IntoContainerPort as _;
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner as _;
use testcontainers::{ContainerAsync, GenericImage, ImageExt as _};

use stepflow_client::local_server::{
    LocalOrchestrator, NatsPluginConfig, OrchestratorConfig, PluginConfig, RouteRule,
};
use stepflow_client::{FlowBuilder, StepflowClient};
use stepflow_worker::{ComponentRegistry, Worker, WorkerConfig};

/// Initialize tracing once for the test process.
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("stepflow_worker=debug".parse().unwrap()),
        )
        .with_test_writer()
        .try_init();
}

/// Check if Docker is available. Returns false if not.
fn docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Start a NATS server container with JetStream enabled.
async fn start_nats() -> (ContainerAsync<GenericImage>, String) {
    let nats = GenericImage::new("nats", "2.11-alpine")
        .with_wait_for(WaitFor::message_on_stderr("Server is ready"))
        .with_exposed_port(4222.tcp())
        .with_cmd(["-js"])
        .start()
        .await
        .expect("Failed to start NATS container");

    let port = nats
        .get_host_port_ipv4(4222)
        .await
        .expect("Failed to get NATS port");

    let url = format!("nats://localhost:{port}");
    // Brief pause for JetStream to fully initialize
    tokio::time::sleep(Duration::from_millis(500)).await;

    (nats, url)
}

/// Build an orchestrator config that uses NATS transport.
fn make_nats_config(nats_url: &str, stream: &str, consumer: &str) -> OrchestratorConfig {
    let mut plugins = HashMap::new();
    plugins.insert("builtin".to_string(), PluginConfig::Builtin);
    plugins.insert(
        "rust-worker".to_string(),
        PluginConfig::Nats(NatsPluginConfig {
            url: nats_url.to_string(),
            stream: Some(stream.to_string()),
            consumer: Some(consumer.to_string()),
            command: None,
            args: vec![],
            env: HashMap::new(),
            queue_timeout_secs: 30,
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

/// Poll `list_components` until `component_id` appears, or panic on timeout.
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
// Test: double a number via NATS transport
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
#[ignore = "requires STEPFLOW_DEV_BINARY (with nats feature) and Docker"]
async fn test_nats_double_component() {
    init_tracing();

    if !docker_available() {
        eprintln!("Docker not available, skipping NATS integration test");
        return;
    }

    let (_nats_container, nats_url) = start_nats().await;

    let orch = LocalOrchestrator::start(make_nats_config(
        &nats_url,
        "RUST_SDK_TEST",
        "rust-test-workers",
    ))
    .await
    .expect("Failed to start local orchestrator");

    // Register the component
    let mut registry = ComponentRegistry::new();
    registry.register_fn("double", |input: DoubleInput, _ctx| async move {
        Ok(DoubleOutput {
            result: input.value * 2,
        })
    });

    // Configure worker to use NATS transport
    let worker_config = WorkerConfig {
        transport: "nats".to_string(),
        nats_url: nats_url.clone(),
        nats_stream: "RUST_SDK_TEST".to_string(),
        nats_consumer: "rust-test-workers".to_string(),
        tasks_url: orch.tasks_url().to_string(),
        orchestrator_url: Some(orch.url().to_string()),
        max_concurrent: 4,
        max_retries: 3,
        shutdown_grace_secs: 5,
        ..Default::default()
    };

    // Run worker in background
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let worker_handle = tokio::spawn(async move {
        Worker::new(registry, worker_config)
            .run_until(async move {
                let _ = stop_rx.await;
            })
            .await
    });

    // Wait for component discovery via NATS
    let mut client = StepflowClient::connect(orch.url())
        .await
        .expect("Failed to connect to orchestrator");
    wait_for_component(&mut client, "double", Duration::from_secs(60)).await;

    // Build and run a flow
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
// Test: chained steps via NATS transport
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
#[ignore = "requires STEPFLOW_DEV_BINARY (with nats feature) and Docker"]
async fn test_nats_chained_steps() {
    init_tracing();

    if !docker_available() {
        eprintln!("Docker not available, skipping NATS integration test");
        return;
    }

    let (_nats_container, nats_url) = start_nats().await;

    let orch = LocalOrchestrator::start(make_nats_config(
        &nats_url,
        "RUST_CHAIN_TEST",
        "rust-chain-workers",
    ))
    .await
    .expect("Failed to start local orchestrator");

    let mut registry = ComponentRegistry::new();
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
        transport: "nats".to_string(),
        nats_url: nats_url.clone(),
        nats_stream: "RUST_CHAIN_TEST".to_string(),
        nats_consumer: "rust-chain-workers".to_string(),
        tasks_url: orch.tasks_url().to_string(),
        orchestrator_url: Some(orch.url().to_string()),
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
