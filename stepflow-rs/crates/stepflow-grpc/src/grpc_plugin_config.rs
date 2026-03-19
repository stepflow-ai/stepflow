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

//! Configuration and plugin factory for gRPC-based pull transport.
//!
//! [`PullPluginConfig`] registers a task queue with the orchestrator's
//! gRPC server ([`StepflowGrpcServer`]) and optionally launches a worker
//! subprocess that connects back via pull transport.
//!
//! [`StepflowGrpcServer`]: crate::grpc_server::StepflowGrpcServer

use std::path::{Path, PathBuf};
use std::sync::Arc;

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::component::ComponentInfo;
use stepflow_core::workflow::{Component, StepId, ValueRef};
use stepflow_plugin::{
    DynPlugin, PluginConfig, PluginError, Result, RunContext, StepflowEnvironment,
};
use tokio::sync::Mutex;

use crate::grpc_server::StepflowGrpcServer;
use crate::in_memory_transport::InMemoryTaskTransport;
use crate::pull_task_queue::PullTaskQueue;
use crate::queue_plugin::StepflowQueuePlugin;
use crate::task_transport::TaskTransport;

/// Transport, queue timeout, and execution timeout — consumed once during initialization.
type TransportInit = (
    Box<dyn TaskTransport>,
    std::time::Duration,
    Option<std::time::Duration>,
);

/// Configuration for a gRPC pull-based plugin.
///
/// When instantiated, creates a [`StepflowQueuePlugin`] backed by an in-memory
/// task queue. During `ensure_initialized`, registers its queue with the
/// orchestrator's [`StepflowGrpcServer`] and optionally spawns a worker
/// subprocess.
///
/// # Config example
///
/// ```yaml
/// plugins:
///   python:
///     type: pull
///     command: uv
///     args: ["--project", "../sdks/python/stepflow-py", "run", "stepflow_py", "--grpc"]
///     queueName: python
/// ```
#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullPluginConfig {
    /// Command to launch the worker subprocess.
    /// If not set, the plugin expects an external worker to connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Arguments for the worker subprocess command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables for the worker subprocess.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub env: std::collections::HashMap<String, String>,

    /// Queue name the worker uses to receive tasks. Required — must match
    /// `STEPFLOW_QUEUE_NAME` in the worker's environment.
    #[serde(default)]
    pub queue_name: Option<String>,

    /// Maximum time (in seconds) a task can wait in the queue for a
    /// worker to send its first heartbeat. If no worker picks up the task
    /// within this window, it is treated as failed. Must be greater than 0.
    ///
    /// Defaults to 30 seconds.
    #[serde(default = "default_queue_timeout_secs")]
    pub queue_timeout_secs: u64,

    /// Maximum time (in seconds) from first heartbeat to `CompleteTask`.
    /// If the worker does not complete within this window, the task is
    /// treated as failed. Heartbeat-based crash detection (5s timeout)
    /// provides faster detection of hard worker crashes.
    ///
    /// Defaults to `null` (no execution timeout — relies on heartbeat
    /// crash detection only).
    #[serde(default)]
    pub execution_timeout_secs: Option<u64>,
}

fn default_queue_timeout_secs() -> u64 {
    30
}

impl PluginConfig for PullPluginConfig {
    type Error = PluginError;

    async fn create_plugin(
        self,
        working_directory: &Path,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, PluginError> {
        if self.queue_timeout_secs == 0 {
            return Err(error_stack::report!(PluginError::Initializing)
                .attach_printable("queue_timeout_secs must be greater than 0"));
        }
        let queue_name = self.queue_name.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing).attach_printable(
                "queueName is required for pull plugins — it identifies this queue \
                 to workers and must match STEPFLOW_QUEUE_NAME in the worker config",
            )
        })?;
        let queue_timeout = std::time::Duration::from_secs(self.queue_timeout_secs);
        let execution_timeout = self
            .execution_timeout_secs
            .map(std::time::Duration::from_secs);
        let queue = Arc::new(PullTaskQueue::new(&queue_name));
        let transport = Box::new(InMemoryTaskTransport::new(queue.clone()));

        let plugin = PullPlugin {
            inner: Mutex::new(None),
            transport_and_timeouts: Mutex::new(Some((transport, queue_timeout, execution_timeout))),
            queue,
            command: self.command,
            args: self.args,
            env: self.env,
            queue_name,
            working_directory: working_directory.to_path_buf(),
            worker: Mutex::new(None),
            server_address: Mutex::new(None),
        };

        Ok(DynPlugin::boxed(plugin))
    }
}

/// Runtime state for the worker subprocess.
///
/// When dropped, the worker subprocess is killed to prevent orphaned
/// processes that spin on "Connection refused" after the orchestrator exits.
/// Uses process group management on Unix to ensure the entire process tree
/// (e.g., `uv` → `python`) is terminated, not just the direct child.
struct WorkerState {
    #[allow(dead_code)] // Kept alive for kill_on_drop behavior
    child: tokio::process::Child,
    /// Process group ID on Unix, used to kill the entire tree.
    #[cfg(unix)]
    pgid: i32,
}

impl Drop for WorkerState {
    fn drop(&mut self) {
        // Kill the entire process group to ensure grandchildren are terminated.
        // start_kill() only kills the direct child, not grandchildren
        // (e.g., python spawned by uv).
        #[cfg(unix)]
        if self.pgid > 0 {
            log::info!("Killing worker process group {}", self.pgid);
            if let Err(e) = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(self.pgid),
                nix::sys::signal::Signal::SIGTERM,
            ) && e != nix::errno::Errno::ESRCH
            {
                log::warn!("Failed to kill worker process group {}: {e}", self.pgid);
            }
        }

        #[cfg(not(unix))]
        {
            let pid = self.child.id();
            if let Err(e) = self.child.start_kill() {
                log::debug!("Failed to kill worker subprocess (pid={pid:?}): {e}");
            }
        }
    }
}

/// gRPC pull-based plugin that registers its task queue with the
/// orchestrator's shared gRPC server and optionally manages a worker
/// subprocess lifecycle.
///
/// Created by [`PullPluginConfig`]. Delegates task execution to
/// [`StepflowQueuePlugin`].
pub struct PullPlugin {
    /// The inner queue plugin, initialized during `ensure_initialized`.
    inner: Mutex<Option<StepflowQueuePlugin>>,
    /// Transport + timeouts consumed during initialization to build the inner plugin.
    transport_and_timeouts: Mutex<Option<TransportInit>>,
    queue: Arc<PullTaskQueue>,
    command: Option<String>,
    args: Vec<String>,
    env: std::collections::HashMap<String, String>,
    queue_name: String,
    working_directory: PathBuf,
    worker: Mutex<Option<WorkerState>>,
    /// Server address stored during initialization, used for subprocess restarts.
    server_address: Mutex<Option<String>>,
}

impl PullPlugin {
    /// Spawn a worker subprocess and wait for it to connect.
    ///
    /// If a previous worker exists, it is dropped first (killing the process
    /// group). The new process inherits the same command, args, env, and
    /// server address from the plugin configuration.
    async fn spawn_worker(&self) -> Result<()> {
        let command = self.command.as_deref().ok_or_else(|| {
            error_stack::report!(PluginError::Execution)
                .attach_printable("Cannot spawn worker: no command configured")
        })?;

        let server_address = self.server_address.lock().await.clone().ok_or_else(|| {
            error_stack::report!(PluginError::Execution)
                .attach_printable("Cannot spawn worker: not initialized (no server address)")
        })?;

        let queue_name = &self.queue_name;

        // Drop old worker first (kills process group via SIGTERM)
        {
            let mut worker = self.worker.lock().await;
            if let Some(old) = worker.take() {
                log::info!("Killing previous worker subprocess");
                drop(old);
            }
        }

        // Capture the next worker generation *before* spawning so we wait
        // specifically for the new worker, not a stale registration from
        // a previous (crashed) connection that hasn't been cleaned up yet.
        let generation = self.queue.next_worker_generation();

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(&self.args)
            .current_dir(&self.working_directory)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            // Set transport config via environment variables so the
            // same worker binary works for all transport modes.
            .env("STEPFLOW_TRANSPORT", "grpc")
            .env("STEPFLOW_TASKS_URL", &server_address)
            .env("STEPFLOW_QUEUE_NAME", queue_name)
            // The blob service is hosted on the same shared gRPC server,
            // so use the same address as the tasks/orchestrator URL.
            .env("STEPFLOW_BLOB_URL", &server_address);

        // Create a new process group on Unix so we can kill the entire
        // tree in Drop. This ensures grandchild processes (e.g., python
        // spawned by uv) are also terminated.
        #[cfg(unix)]
        // SAFETY: setpgid before exec is safe and standard practice.
        unsafe {
            cmd.pre_exec(|| {
                nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
                    .map_err(std::io::Error::other)?;
                Ok(())
            });
        }

        for (k, v) in &self.env {
            cmd.env(k, v);
        }

        let child = cmd
            .spawn()
            .change_context(PluginError::Execution)
            .attach_printable_lazy(|| format!("Failed to spawn gRPC worker: {command}"))?;

        #[cfg(unix)]
        let pgid = child.id().map(|pid| pid as i32).unwrap_or(0);

        log::info!(
            "gRPC worker subprocess spawned (pid={:?}, queue={queue_name})",
            child.id()
        );

        // Wrap in WorkerState immediately so the process group is killed
        // on all exit paths (including timeout errors below).
        let worker_state = WorkerState {
            child,
            #[cfg(unix)]
            pgid,
        };

        // Wait for the *new* worker to connect (not a stale registration).
        let connected = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.queue.wait_for_worker_since(generation),
        )
        .await;

        if connected.is_err() {
            // worker_state is dropped here, killing the process group.
            log::error!("gRPC worker did not connect within 30 seconds");
            return Err(error_stack::report!(PluginError::Execution)
                .attach_printable("gRPC worker did not connect within 30 seconds"));
        }

        log::info!("gRPC worker connected successfully");
        *self.worker.lock().await = Some(worker_state);

        Ok(())
    }
}

impl stepflow_plugin::Plugin for PullPlugin {
    async fn ensure_initialized(&self, env: &Arc<StepflowEnvironment>) -> Result<()> {
        // Idempotent: skip if already initialized
        {
            let inner = self.inner.lock().await;
            if inner.is_some() {
                return Ok(());
            }
        }

        // Get the shared gRPC server from the environment.
        let shared_server = env.get::<Arc<StepflowGrpcServer>>().ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("StepflowGrpcServer not found in environment")
        })?;

        // Get the server address (set by StepflowService during startup).
        let server_address = shared_server.address().await.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("gRPC server address not set — StepflowService must be created before pull plugins are initialized")
        })?;

        // Store server address for subprocess restarts
        *self.server_address.lock().await = Some(server_address.clone());

        // Register this plugin's queue with the shared server
        shared_server.register_queue(self.queue_name.clone(), self.queue.clone());

        // Build the inner StepflowQueuePlugin with the shared PendingTasks
        let (transport, queue_timeout, execution_timeout) = self
            .transport_and_timeouts
            .lock()
            .await
            .take()
            .ok_or_else(|| {
                error_stack::report!(PluginError::Initializing)
                    .attach_printable("transport already consumed (double initialization?)")
            })?;

        let inner_plugin = StepflowQueuePlugin::new(
            transport,
            shared_server.pending_tasks().clone(),
            queue_timeout,
            execution_timeout,
        );

        // Set the orchestrator URL so task assignments carry the gRPC server address.
        //
        // In `serve` mode, the gRPC server is multiplexed on the same port as HTTP,
        // so server_address matches the environment's OrchestratorServiceUrl.
        //
        // In `run`/`test` mode, the PullPlugin starts its own gRPC server on a
        // dynamic port, separate from the HTTP blob API server. The server_address
        // is the gRPC server's actual bind address — we must use it, not the
        // environment's OrchestratorServiceUrl (which points to the HTTP port).
        //
        // STEPFLOW_ORCHESTRATOR_URL overrides for K8s where workers need the
        // service DNS name rather than the local bind address.
        let advertised_address =
            std::env::var("STEPFLOW_ORCHESTRATOR_URL").unwrap_or(server_address.clone());
        inner_plugin.set_orchestrator_url(advertised_address);

        *self.inner.lock().await = Some(inner_plugin);

        // Spawn worker subprocess if command is configured
        if self.command.is_some() {
            self.spawn_worker().await?;
        }

        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        let inner = self.inner.lock().await;
        let inner = inner.as_ref().ok_or_else(|| {
            error_stack::report!(PluginError::Execution).attach_printable("plugin not initialized")
        })?;
        inner.list_components().await
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let inner = self.inner.lock().await;
        let inner = inner.as_ref().ok_or_else(|| {
            error_stack::report!(PluginError::Execution).attach_printable("plugin not initialized")
        })?;
        inner.component_info(component).await
    }

    async fn start_task(
        &self,
        task_id: &str,
        component: &Component,
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
        input: ValueRef,
        attempt: u32,
    ) -> Result<()> {
        let inner = self.inner.lock().await;
        let inner = inner.as_ref().ok_or_else(|| {
            error_stack::report!(PluginError::Execution).attach_printable("plugin not initialized")
        })?;
        inner
            .start_task(task_id, component, run_context, step, input, attempt)
            .await
    }

    async fn prepare_for_retry(&self) -> Result<()> {
        if self.command.is_some() {
            log::info!("Restarting pull worker subprocess for retry");
            self.spawn_worker().await?;
        }
        // Remote mode (no command): no-op — external orchestration handles restart.
        Ok(())
    }
}
