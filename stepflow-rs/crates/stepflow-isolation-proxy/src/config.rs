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

//! Proxy configuration — CLI args and environment variables.

use clap::{Parser, Subcommand};

/// Stepflow isolation proxy — dispatches tasks to sandboxed workers via vsock.
///
/// Pulls tasks from the orchestrator and dispatches them to isolated workers
/// using a pluggable backend (subprocess, Firecracker, etc.). The worker
/// receives tasks over vsock/Unix socket and handles the full task lifecycle
/// (claim, heartbeat, execute, complete) by talking directly to the orchestrator.
#[derive(Parser, Debug)]
#[command(name = "stepflow-isolation-proxy")]
#[command(about = "Dispatch Stepflow tasks to isolated sandboxed workers")]
pub struct ProxyConfig {
    /// Tasks gRPC service URL (PullTasks endpoint).
    #[arg(
        long,
        env = "STEPFLOW_TASKS_URL",
        default_value = "http://127.0.0.1:7837"
    )]
    pub tasks_url: String,

    /// Queue name to pull tasks for.
    #[arg(long, env = "STEPFLOW_QUEUE_NAME", default_value = "default")]
    pub queue_name: String,

    /// Blob API URL passed to workers.
    #[arg(long, env = "STEPFLOW_BLOB_URL", default_value = "")]
    pub blob_url: String,

    /// OpenTelemetry collector endpoint passed to workers.
    #[arg(long, env = "OTEL_EXPORTER_OTLP_ENDPOINT", default_value = "")]
    pub otel_endpoint: String,

    /// Maximum number of concurrent tasks / sandboxes.
    #[arg(long, env = "STEPFLOW_MAX_CONCURRENT", default_value = "4")]
    pub max_concurrent: usize,

    /// Isolation backend to use.
    #[command(subcommand)]
    pub backend: Backend,
}

/// Isolation backend — how tasks are dispatched to workers.
#[derive(Subcommand, Debug)]
pub enum Backend {
    /// Subprocess isolation: spawn a Python worker per task, communicate via Unix socket.
    /// Works on all platforms. Good for development, testing, and lightweight isolation.
    Subprocess {
        /// Path to the Python executable (must be a single executable, not a shell command).
        #[arg(long, env = "STEPFLOW_PYTHON", default_value = "python3")]
        python: String,

        /// Custom worker script. If set, runs `<python> <script> --socket <path> --oneshot`.
        /// Default: `python3 -m stepflow_py.worker.vsock_worker`.
        #[arg(long, env = "STEPFLOW_WORKER_SCRIPT")]
        worker_script: Option<String>,
    },

    /// Firecracker microVM isolation: dispatch tasks to Firecracker VMs via vsock.
    /// Requires Linux with KVM (or Lima on macOS). Provides hardware-level isolation.
    Firecracker {
        /// Path to the Firecracker-compatible kernel (vmlinux.bin).
        #[arg(
            long,
            env = "STEPFLOW_FC_KERNEL",
            default_value = "./build/vmlinux.bin"
        )]
        kernel: String,

        /// Path to the root filesystem image (ext4).
        #[arg(
            long,
            env = "STEPFLOW_FC_ROOTFS",
            default_value = "./build/stepflow-rootfs.ext4"
        )]
        rootfs: String,

        /// Number of vCPUs per VM.
        #[arg(long, env = "STEPFLOW_FC_VCPUS", default_value = "1")]
        vcpus: u32,

        /// Memory per VM in MiB.
        #[arg(long, env = "STEPFLOW_FC_MEMORY_MB", default_value = "256")]
        memory_mb: u32,

        /// Number of pre-warmed VMs to keep ready.
        #[arg(long, env = "STEPFLOW_FC_POOL_SIZE", default_value = "2")]
        pool_size: usize,

        /// Disable snapshot-based restore (boot each VM from scratch).
        #[arg(long, env = "STEPFLOW_FC_NO_SNAPSHOT")]
        no_snapshot: bool,
    },
}
