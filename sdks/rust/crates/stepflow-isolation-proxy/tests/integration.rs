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

//! Integration tests for the Firecracker proxy.
//!
//! These tests verify the full dev-mode pipeline:
//! Orchestrator → Proxy (PullTasks) → Unix socket → Python vsock worker → CompleteTask
//!
//! # Requirements
//!
//! - `STEPFLOW_DEV_BINARY`: Path to the `stepflow-server` binary.
//! - `uv` on PATH with the Python SDK (`stepflow-py`) project available.
//!
//! # Running
//!
//! ```bash
//! cd stepflow-rs
//! cargo build -p stepflow-server --no-default-features
//!
//! STEPFLOW_DEV_BINARY=$(pwd)/target/debug/stepflow-server \
//!   cargo test -p stepflow-isolation-proxy --test integration -- --include-ignored
//! ```

use std::collections::HashMap;
use std::time::Duration;

use stepflow_client::local_server::{
    GrpcPluginConfig, LocalOrchestrator, OrchestratorConfig, PluginConfig, RouteRule,
};
use stepflow_client::{FlowBuilder, StepflowClient, ValueExpr};
use stepflow_proto::VsockTaskEnvelope;
use stepflow_worker::vsock::write_message;

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("stepflow_isolation_proxy=debug".parse().unwrap())
                .add_directive("stepflow_worker=debug".parse().unwrap()),
        )
        .with_test_writer()
        .try_init();
}

/// Build orchestrator config with builtin + a pull-based "firecracker" plugin.
fn make_config(queue_name: &str) -> OrchestratorConfig {
    let mut plugins = HashMap::new();
    plugins.insert("builtin".to_string(), PluginConfig::Builtin);
    plugins.insert(
        "firecracker".to_string(),
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
        "/fc".to_string(),
        vec![RouteRule {
            plugin: "firecracker".to_string(),
        }],
    );

    OrchestratorConfig {
        plugins,
        routes,
        storage_config: None,
    }
}

fn test_worker_script() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{manifest_dir}/tests/test_worker.py")
}

fn stepflow_py_project_dir() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{manifest_dir}/../../../python/stepflow-py")
}

// ---------------------------------------------------------------------------
// Test: End-to-end dev mode — proxy dispatches task to Python worker via socket
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires STEPFLOW_DEV_BINARY — run with --include-ignored"]
async fn test_dev_mode_double() {
    init_tracing();
    const QUEUE: &str = "test-fc-double";

    // 1. Start local orchestrator
    let orch = LocalOrchestrator::start(make_config(QUEUE))
        .await
        .expect("Failed to start local orchestrator");

    let tasks_url = orch.tasks_url().to_string();

    // 2. Start the Python test worker and measure startup time (dep loading).
    let socket_path = format!("/tmp/stepflow-test-{}.sock", uuid::Uuid::new_v4());
    let py_project = stepflow_py_project_dir();
    let worker_script = test_worker_script();

    let env_start = std::time::Instant::now();
    let child = tokio::process::Command::new("uv")
        .args([
            "run",
            "--project",
            &py_project,
            "python",
            &worker_script,
            "--socket",
            &socket_path,
            "--oneshot",
        ])
        .env("STEPFLOW_LOG_LEVEL", "DEBUG")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("Failed to spawn Python test worker");

    // Wait for the socket to appear (STEPFLOW_VSOCK_READY)
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if tokio::fs::metadata(&socket_path).await.is_ok() {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("Timed out waiting for Python worker socket at {socket_path}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let env_ready = env_start.elapsed();
    eprintln!("TIMING: subprocess env_create={env_ready:?} (spawn + dep load + listen)");

    // 3. Connect to the worker socket (but don't send anything yet)
    let mut conn = tokio::net::UnixStream::connect(&socket_path)
        .await
        .expect("Failed to connect to Python worker socket");

    // 4. Open PullTasks stream
    let worker_id = format!("test-proxy-{}", uuid::Uuid::new_v4());
    let (_channel, mut stream) = stepflow_worker::open_task_stream(&tasks_url, QUEUE, &worker_id)
        .await
        .expect("Failed to open task stream");

    // 5. Store and submit a flow that routes to /fc/double
    let mut client = StepflowClient::connect(orch.url())
        .await
        .expect("Failed to connect to orchestrator");

    let mut builder = FlowBuilder::new();
    builder.add_step(
        "double",
        "/fc/double",
        ValueExpr::workflow_input(Default::default()),
    );
    let flow = builder
        .output(ValueExpr::step_output("double"))
        .build()
        .expect("Failed to build flow");

    let flow_id = client
        .store_flow(&flow)
        .await
        .expect("Failed to store flow");

    let submit_time = std::time::Instant::now();
    let run_id = client
        .submit(&flow_id, serde_json::json!({"value": 21}))
        .await
        .expect("Failed to submit flow");

    // 6. Pull the task from the stream
    let assignment = tokio::time::timeout(Duration::from_secs(10), stream.message())
        .await
        .expect("Timed out waiting for task")
        .expect("Stream error")
        .expect("Stream ended unexpectedly");

    assert!(!assignment.task_id.is_empty());
    assert!(assignment.task.is_some());

    // 7. Send the VsockTaskEnvelope to the already-waiting Python worker.
    //    The worker will immediately claim the task via heartbeat.
    let envelope = VsockTaskEnvelope {
        assignment: Some(assignment),
        blob_url: String::new(),
        tasks_url: tasks_url.clone(),
        otel_endpoint: String::new(),
    };

    write_message(&mut conn, &envelope)
        .await
        .expect("Failed to send envelope");

    // Drop the connection to signal end-of-stream.
    drop(conn);

    // 8. Wait for the Python worker to exit
    let output = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
        .await
        .expect("Timed out waiting for Python worker to exit")
        .expect("Failed to wait for Python worker");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("=== Python worker stdout ===\n{stdout}");
    eprintln!("=== Python worker stderr ===\n{stderr}");

    assert!(
        output.status.success(),
        "Python worker exited with: {}",
        output.status
    );

    // 9. Verify the run completed with the expected result
    let run_status = client
        .get_run(&run_id, true)
        .await
        .expect("Failed to get run status");

    let complete_time = submit_time.elapsed();

    // Status 2 = Completed
    assert_eq!(
        run_status.status, 2,
        "Expected run to be completed (2), got {}",
        run_status.status
    );

    eprintln!("TIMING: subprocess env_create={env_ready:?} (NOT pre-warmed, paid per task)");
    eprintln!(
        "TIMING: subprocess execute_and_complete={complete_time:?} (submit → orchestrator reports done)"
    );
    eprintln!(
        "TIMING: subprocess total_per_task={:?} (env_create + execute)",
        env_ready + complete_time
    );

    // Fetch outputs separately (get_run doesn't include them)
    let items = client
        .get_run_items(&run_id)
        .await
        .expect("Failed to get run items");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0], serde_json::json!({"result": 42}));

    // Cleanup
    let _ = tokio::fs::remove_file(&socket_path).await;
}
