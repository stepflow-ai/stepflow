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
use stepflow_core::FlowResult;
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
        let queue = Arc::new(PullTaskQueue::new());
        let transport = Box::new(InMemoryTaskTransport::new(queue.clone()));

        let plugin = PullPlugin {
            inner: Mutex::new(None),
            transport_and_timeouts: Mutex::new(Some((transport, queue_timeout, execution_timeout))),
            queue,
            command: self.command,
            args: self.args,
            env: self.env,
            queue_name: self.queue_name,
            working_directory: working_directory.to_path_buf(),
            worker: Mutex::new(None),
        };

        Ok(DynPlugin::boxed(plugin))
    }
}

/// Runtime state for the worker subprocess.
///
/// When dropped, the worker subprocess is killed to prevent orphaned
/// processes that spin on "Connection refused" after the orchestrator exits.
struct WorkerState {
    child: tokio::process::Child,
}

impl Drop for WorkerState {
    fn drop(&mut self) {
        let pid = self.child.id();
        if let Err(e) = self.child.start_kill() {
            log::debug!("Failed to kill worker subprocess (pid={pid:?}): {e}");
        } else {
            log::debug!("Killed worker subprocess (pid={pid:?})");
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
    queue_name: Option<String>,
    working_directory: PathBuf,
    worker: Mutex<Option<WorkerState>>,
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

        // Get or create the shared gRPC server from the environment.
        // The first plugin to initialize will start the server.
        let shared_server = env.get::<Arc<StepflowGrpcServer>>().ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("StepflowGrpcServer not found in environment")
        })?;

        // Start the server (idempotent — returns existing address if already running)
        let server_address = shared_server.ensure_started(env).await?;

        // Register this plugin's queue with the shared server
        let queue_name = self.queue_name.as_deref().unwrap_or("python");
        shared_server.register_queue(queue_name.to_string(), self.queue.clone());

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

        // Set the orchestrator URL so task assignments carry the shared server address
        inner_plugin.set_orchestrator_url(server_address.clone());

        *self.inner.lock().await = Some(inner_plugin);

        // Spawn worker subprocess if command is configured
        if let Some(command) = &self.command {
            let mut cmd = tokio::process::Command::new(command);
            cmd.args(&self.args)
                .current_dir(&self.working_directory)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                // Set transport config via environment variables so the
                // same worker binary works for all transport modes.
                .env("STEPFLOW_TRANSPORT", "grpc")
                .env("STEPFLOW_TASKS_URL", &server_address)
                .env("STEPFLOW_QUEUE_NAME", queue_name)
                // The blob service is hosted on the same shared gRPC server,
                // so use the same address as the tasks/orchestrator URL.
                .env("STEPFLOW_BLOB_URL", &server_address);

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
            *self.worker.lock().await = Some(WorkerState { child });
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

    async fn execute(
        &self,
        component: &Component,
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
        input: ValueRef,
        attempt: u32,
    ) -> Result<FlowResult> {
        let inner = self.inner.lock().await;
        let inner = inner.as_ref().ok_or_else(|| {
            error_stack::report!(PluginError::Execution).attach_printable("plugin not initialized")
        })?;
        inner
            .execute(component, run_context, step, input, attempt)
            .await
    }

    async fn prepare_for_retry(&self) -> Result<()> {
        let inner = self.inner.lock().await;
        let inner = inner.as_ref().ok_or_else(|| {
            error_stack::report!(PluginError::Execution).attach_printable("plugin not initialized")
        })?;
        inner.prepare_for_retry().await
    }
}
