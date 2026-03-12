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
//! [`PullPluginConfig`] spawns a gRPC server (TasksService + OrchestratorService)
//! and optionally launches a worker subprocess that connects back via pull transport.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::FlowResult;
use stepflow_core::component::ComponentInfo;
use stepflow_core::workflow::{Component, StepId, ValueRef};
use stepflow_plugin::{
    DynPlugin, PluginConfig, PluginError, Result, RunContext, StepflowEnvironment,
};
use tokio::sync::Mutex;

use crate::in_memory_transport::InMemoryTaskTransport;
use crate::pending_tasks::PendingTasks;
use crate::proto::stepflow::v1::orchestrator_service_server::OrchestratorServiceServer;
use crate::proto::stepflow::v1::tasks_service_server::TasksServiceServer;
use crate::pull_task_queue::PullTaskQueue;
use crate::queue_plugin::StepflowQueuePlugin;
use crate::services::{OrchestratorServiceImpl, TasksServiceImpl};

/// Configuration for a gRPC pull-based plugin.
///
/// When instantiated, creates a [`StepflowQueuePlugin`] backed by an in-memory
/// task queue. The gRPC server and optional worker subprocess are started
/// during `ensure_initialized`, which runs after the environment is built.
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

    /// Queue name the worker uses to receive tasks (matches plugin key in orchestrator config).
    /// Defaults to the plugin's key name in the config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_name: Option<String>,

    /// Maximum time (in seconds) a task can wait in the queue for a
    /// worker to call `StartTask`. If no worker picks up the task within
    /// this window, it is treated as failed. Must be greater than 0.
    ///
    /// Defaults to 30 seconds.
    #[serde(default = "default_queue_timeout_secs")]
    pub queue_timeout_secs: u64,

    /// Maximum time (in seconds) from `StartTask` to `CompleteTask`.
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
        let queue_timeout = std::time::Duration::from_secs(self.queue_timeout_secs);
        let execution_timeout = self
            .execution_timeout_secs
            .map(std::time::Duration::from_secs);
        let pending_tasks = PendingTasks::new();
        let queue = Arc::new(PullTaskQueue::new());
        let transport = Box::new(InMemoryTaskTransport::new(queue.clone()));
        let inner = StepflowQueuePlugin::new(
            transport,
            pending_tasks.clone(),
            queue_timeout,
            execution_timeout,
        );

        let plugin = PullPlugin {
            inner,
            queue,
            pending_tasks,
            command: self.command,
            args: self.args,
            env: self.env,
            queue_name: self.queue_name,
            working_directory: working_directory.to_path_buf(),
            state: Mutex::new(None),
        };

        Ok(DynPlugin::boxed(plugin))
    }
}

/// Runtime state created during initialization.
///
/// When dropped, the gRPC server task is aborted and the worker subprocess
/// is killed. This prevents orphaned worker processes that spin on
/// "Connection refused" after the orchestrator exits.
struct PullPluginState {
    /// Worker subprocess handle — killed on drop.
    worker: Option<tokio::process::Child>,
    /// gRPC server task handle — aborted on drop.
    server: tokio::task::JoinHandle<()>,
}

impl Drop for PullPluginState {
    fn drop(&mut self) {
        // Abort the gRPC server task so the listener is closed.
        self.server.abort();

        // Kill the worker subprocess to prevent orphaned processes.
        if let Some(ref mut child) = self.worker {
            let pid = child.id();
            if let Err(e) = child.start_kill() {
                log::debug!("Failed to kill worker subprocess (pid={pid:?}): {e}");
            } else {
                log::debug!("Killed worker subprocess (pid={pid:?})");
            }
        }
    }
}

/// gRPC pull-based plugin that manages the lifecycle of a gRPC server and
/// optional worker subprocess.
///
/// Created by [`PullPluginConfig`]. Delegates task execution to
/// [`StepflowQueuePlugin`].
pub struct PullPlugin {
    inner: StepflowQueuePlugin,
    queue: Arc<PullTaskQueue>,
    pending_tasks: Arc<PendingTasks>,
    command: Option<String>,
    args: Vec<String>,
    env: std::collections::HashMap<String, String>,
    queue_name: Option<String>,
    working_directory: PathBuf,
    state: Mutex<Option<PullPluginState>>,
}

impl stepflow_plugin::Plugin for PullPlugin {
    async fn ensure_initialized(&self, env: &Arc<StepflowEnvironment>) -> Result<()> {
        // Idempotent: skip if already initialized
        let mut state = self.state.lock().await;
        if state.is_some() {
            return Ok(());
        }

        // Start gRPC server on a random port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .change_context(PluginError::Initializing)
            .attach_printable("Failed to bind gRPC server")?;
        let port = listener
            .local_addr()
            .change_context(PluginError::Initializing)?
            .port();

        let orchestrator_service =
            OrchestratorServiceImpl::new(env.clone(), self.pending_tasks.clone());
        let tasks_service = TasksServiceImpl::new(self.queue.clone());

        let server_handle = tokio::spawn(async move {
            let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
            if let Err(e) = tonic::transport::Server::builder()
                .add_service(OrchestratorServiceServer::new(orchestrator_service))
                .add_service(TasksServiceServer::new(tasks_service))
                .serve_with_incoming(incoming)
                .await
            {
                log::error!("gRPC server error: {e}");
            }
        });

        log::info!("gRPC pull transport server started on port {port}");

        // Set the orchestrator URL override so task assignments carry the
        // correct URL for workers to call CompleteTask back to
        self.inner.set_orchestrator_url(format!("127.0.0.1:{port}"));

        // Spawn worker subprocess if command is configured
        let worker = if let Some(command) = &self.command {
            let orchestrator_url = format!("127.0.0.1:{port}");
            let queue_name = self.queue_name.as_deref().unwrap_or("python");

            let mut cmd = tokio::process::Command::new(command);
            cmd.args(&self.args)
                .current_dir(&self.working_directory)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                // Set transport config via environment variables so the
                // same worker binary works for all transport modes.
                .env("STEPFLOW_TRANSPORT", "grpc")
                .env("STEPFLOW_TASKS_URL", &orchestrator_url)
                .env("STEPFLOW_QUEUE_NAME", queue_name);

            for (k, v) in &self.env {
                cmd.env(k, v);
            }

            let child = cmd
                .spawn()
                .change_context(PluginError::Initializing)
                .attach_printable_lazy(|| format!("Failed to spawn gRPC worker: {command}"))?;

            log::info!(
                "gRPC worker subprocess spawned (pid={:?}, queue={queue_name})",
                child.id()
            );

            // Wait for the worker to connect
            let connected = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                self.queue.wait_for_worker(),
            )
            .await;

            if connected.is_err() {
                log::error!("gRPC worker did not connect within 30 seconds");
                return Err(error_stack::report!(PluginError::Initializing)
                    .attach_printable("gRPC worker did not connect within 30 seconds"));
            }

            log::info!("gRPC worker connected successfully");
            Some(child)
        } else {
            None
        };

        *state = Some(PullPluginState {
            worker,
            server: server_handle,
        });

        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        self.inner.list_components().await
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        self.inner.component_info(component).await
    }

    async fn execute(
        &self,
        component: &Component,
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
        input: ValueRef,
        attempt: u32,
    ) -> Result<FlowResult> {
        self.inner
            .execute(component, run_context, step, input, attempt)
            .await
    }

    async fn prepare_for_retry(&self) -> Result<()> {
        self.inner.prepare_for_retry().await
    }
}
