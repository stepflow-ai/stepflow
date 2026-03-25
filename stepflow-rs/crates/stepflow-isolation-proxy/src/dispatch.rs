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

//! Task dispatch loop — pulls tasks and forwards them to sandboxed workers.

use std::sync::Arc;
use std::time::Duration;

use log::{error, info};
use stepflow_proto::{TaskAssignment, VsockTaskEnvelope};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

use crate::config::{Backend, ProxyConfig};
use crate::subprocess;
use crate::vsock::protocol::write_message;
use crate::vsock::transport::VsockConnection;

/// Run the main dispatch loop.
///
/// Pulls tasks from the orchestrator via `open_task_stream` and dispatches
/// each to a sandboxed worker using the configured backend.
pub async fn run(config: ProxyConfig) -> Result<(), Box<dyn std::error::Error>> {
    let worker_id = format!("stepflow-proxy-{}", uuid::Uuid::new_v4());
    info!(
        "Starting proxy worker_id={worker_id} backend={}",
        backend_name(&config.backend)
    );
    info!(
        "Connecting to tasks service at {} queue={}",
        config.tasks_url, config.queue_name
    );

    let (_channel, mut stream) =
        stepflow_worker::open_task_stream(&config.tasks_url, &config.queue_name, &worker_id)
            .await?;

    info!("Connected. Waiting for tasks...");

    let semaphore = Arc::new(Semaphore::new(config.max_concurrent));
    let config = Arc::new(config);

    loop {
        let assignment = match stream.message().await {
            Ok(Some(assignment)) => assignment,
            Ok(None) => {
                info!("Task stream ended (orchestrator closed)");
                break;
            }
            Err(e) => {
                error!("Task stream error: {e}");
                // TODO: reconnect with backoff
                break;
            }
        };

        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let config = Arc::clone(&config);

        tokio::spawn(async move {
            let task_id = assignment.task_id.clone();
            info!("Dispatching task {task_id}");

            let start = std::time::Instant::now();
            if let Err(e) = dispatch_task(&config, assignment).await {
                error!("Task {task_id} dispatch failed: {e}");
            }
            let elapsed = start.elapsed();
            info!("Task {task_id} completed in {elapsed:?}");

            drop(permit);
        });
    }

    Ok(())
}

/// Dispatch a single task using the configured backend.
async fn dispatch_task(
    config: &ProxyConfig,
    assignment: TaskAssignment,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let envelope = VsockTaskEnvelope {
        assignment: Some(assignment),
        blob_url: config.blob_url.clone(),
        tasks_url: config.tasks_url.clone(),
        otel_endpoint: config.otel_endpoint.clone(),
    };

    match &config.backend {
        Backend::Subprocess {
            python,
            worker_script,
        } => dispatch_subprocess(python, worker_script.as_deref(), &envelope).await,
        Backend::Firecracker { .. } => {
            Err("Firecracker backend not yet implemented — use 'subprocess' for now".into())
        }
    }
}

/// Subprocess backend: spawn a Python worker, connect via Unix socket, send task.
async fn dispatch_subprocess(
    python: &str,
    worker_script: Option<&str>,
    envelope: &VsockTaskEnvelope,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let socket_path = format!("/tmp/stepflow-vsock-{}.sock", uuid::Uuid::new_v4());

    let mut child = subprocess::spawn_python_worker(python, &socket_path, worker_script).await?;

    subprocess::wait_for_socket(&socket_path, Duration::from_secs(10)).await?;

    let mut conn = VsockConnection::connect(&socket_path).await?;
    write_message(&mut conn.stream, envelope).await?;

    // Close write side to signal end-of-stream (oneshot)
    conn.stream.shutdown().await?;

    let status = child.wait().await?;

    // Clean up socket file regardless of outcome
    let _ = tokio::fs::remove_file(&socket_path).await;

    if !status.success() {
        return Err(format!("Python worker exited with status: {status}").into());
    }

    Ok(())
}

fn backend_name(backend: &Backend) -> &'static str {
    match backend {
        Backend::Subprocess { .. } => "subprocess",
        Backend::Firecracker { .. } => "firecracker",
    }
}
