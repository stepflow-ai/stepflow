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

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use log::{error, info};
use stepflow_proto::tasks_service_client::TasksServiceClient;
use stepflow_proto::{PullTasksRequest, TaskAssignment, VsockTaskEnvelope};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tonic::codec::Streaming;

use crate::config::{Backend, ProxyConfig};
use crate::subprocess;
use crate::vm::lifecycle::VmConfig;
use crate::vm::pool::VmPool;
use crate::vsock::protocol::write_message;
use crate::vsock::transport::VsockConnection;

/// Run the main dispatch loop.
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

    // Initialize the Firecracker VM pool if using that backend.
    let vm_pool = match &config.backend {
        Backend::Firecracker {
            kernel,
            rootfs,
            vcpus,
            memory_mb,
            pool_size,
            no_snapshot,
            jailer,
            uid,
            gid,
        } => {
            let vm_config = VmConfig {
                kernel_path: PathBuf::from(kernel),
                rootfs_path: PathBuf::from(rootfs),
                vcpu_count: *vcpus,
                mem_size_mib: *memory_mb,
                jailer_path: PathBuf::from(jailer),
                jail_uid: *uid,
                jail_gid: *gid,
                ..Default::default()
            };
            let pool = VmPool::new(vm_config, *pool_size, !no_snapshot).await?;
            Some(Arc::new(pool))
        }
        _ => None,
    };

    let mut stream = open_task_stream(&config.tasks_url, &config.queue_name, &worker_id).await?;

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
                break;
            }
        };

        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let config = Arc::clone(&config);
        let vm_pool = vm_pool.clone();

        tokio::spawn(async move {
            let task_id = assignment.task_id.clone();
            let task_received = std::time::Instant::now();

            if let Err(e) = dispatch_task(&config, assignment, vm_pool.as_deref()).await {
                error!("Task {task_id} dispatch failed: {e}");
            }
            info!(
                "TIMING: task_id={task_id} e2e_proxy={:?} (received → worker done)",
                task_received.elapsed()
            );

            drop(permit);
        });
    }

    Ok(())
}

/// Dispatch a single task using the configured backend.
async fn dispatch_task(
    config: &ProxyConfig,
    assignment: TaskAssignment,
    vm_pool: Option<&VmPool>,
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
            dispatch_firecracker(vm_pool.expect("VM pool not initialized"), &envelope).await
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

/// Firecracker backend: take a pre-warmed VM, send task via vsock, wait for exit.
async fn dispatch_firecracker(
    pool: &VmPool,
    envelope: &VsockTaskEnvelope,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Take a ready VM from the pool (may block until one is available)
    let t0 = std::time::Instant::now();

    let vm = pool
        .take()
        .await
        .map_err(|e| format!("Failed to get VM: {e}"))?;
    let t_pool = t0.elapsed();

    // Rewrite localhost URLs so the VM can reach host services via its unique veth IP.
    let vm_envelope = rewrite_urls_for_vm(envelope, &vm.host_ip);

    // Connect to the VM's vsock and send the task.
    let result = dispatch_to_vsock_vm(&vm.vsock_uds_path, 5000, &vm_envelope).await;
    let t_total = t0.elapsed();

    if let Err(ref e) = result {
        error!("VM {} dispatch failed: {e}", vm.vm_id);
    } else {
        info!(
            "TIMING: pool_take={:?} total_dispatch={:?} (vm={})",
            t_pool, t_total, vm.vm_id
        );
    }

    vm.shutdown().await;
    result
}

/// Rewrite localhost URLs in the envelope to be reachable from inside the VM.
///
/// The VM uses its veth default route to reach the host. Traffic to the
/// host-side veth IP (10.0.0.1) exits the namespace and reaches host services.
/// We can't use the TAP IP (192.168.241.1) because that's local to the namespace.
fn rewrite_urls_for_vm(envelope: &VsockTaskEnvelope, host_ip: &str) -> VsockTaskEnvelope {
    let rewrite = |url: &str| -> String {
        url.replace("127.0.0.1", host_ip)
            .replace("localhost", host_ip)
    };

    let mut rewritten = envelope.clone();
    rewritten.blob_url = rewrite(&rewritten.blob_url);
    rewritten.tasks_url = rewrite(&rewritten.tasks_url);
    rewritten.otel_endpoint = rewrite(&rewritten.otel_endpoint);

    // Also rewrite the orchestrator URL inside the TaskContext
    if let Some(ref mut assignment) = rewritten.assignment
        && let Some(ref mut ctx) = assignment.context
    {
        ctx.orchestrator_service_url = rewrite(&ctx.orchestrator_service_url);
    }

    rewritten
}

/// Send a task envelope to a pre-warmed Firecracker VM worker via vsock.
///
/// The VM is already listening (readiness verified during pool warming).
/// Sends the envelope, then waits for the worker to close its end of the
/// connection (indicating CompleteTask was sent to the orchestrator).
async fn dispatch_to_vsock_vm(
    vsock_uds_path: &str,
    guest_port: u32,
    envelope: &VsockTaskEnvelope,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::AsyncReadExt;

    let t0 = std::time::Instant::now();

    // The VM is pre-warmed by the pool — guest worker should already be listening.
    let mut conn = VsockConnection::connect_vsock(vsock_uds_path, guest_port).await?;
    let t_connect = t0.elapsed();

    write_message(&mut conn.stream, envelope).await?;
    let t_send = t0.elapsed();

    // Close write side to signal end-of-stream
    conn.stream.shutdown().await?;

    // Wait for the worker to close its end of the connection (EOF).
    let mut buf = [0u8; 64];
    loop {
        match conn.stream.read(&mut buf).await {
            Ok(0) | Err(_) => break, // EOF or error — worker is done
            Ok(_) => continue,       // Unexpected data — keep draining
        }
    }
    let t_done = t0.elapsed();

    info!(
        "TIMING: vsock_connect={:?} send={:?} worker_done={:?}",
        t_connect,
        t_send - t_connect,
        t_done - t_send,
    );

    Ok(())
}

fn backend_name(backend: &Backend) -> &'static str {
    match backend {
        Backend::Subprocess { .. } => "subprocess",
        Backend::Firecracker { .. } => "firecracker",
    }
}

/// Connect to the tasks service and open a `PullTasks` stream.
async fn open_task_stream(
    tasks_url: &str,
    queue_name: &str,
    worker_id: &str,
) -> Result<Streaming<TaskAssignment>, Box<dyn std::error::Error>> {
    let channel = tonic::transport::Channel::from_shared(tasks_url.to_string())
        .map_err(|e| format!("invalid tasks URL '{tasks_url}': {e}"))?
        .connect()
        .await
        .map_err(|e| format!("failed to connect to tasks service at '{tasks_url}': {e}"))?;

    let stream = TasksServiceClient::new(channel)
        .pull_tasks(PullTasksRequest {
            queue_name: queue_name.to_string(),
            worker_id: worker_id.to_string(),
        })
        .await
        .map_err(|e| format!("failed to open PullTasks stream: {e}"))?
        .into_inner();

    Ok(stream)
}
