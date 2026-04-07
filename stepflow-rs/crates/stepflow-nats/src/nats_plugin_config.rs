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

//! Configuration and plugin factory for NATS JetStream transport.
//!
//! [`NatsPluginConfig`] connects to a NATS server and dispatches tasks via
//! JetStream. Workers consume tasks from NATS and report completion via
//! the gRPC `OrchestratorService` (URL embedded in each task's `TaskContext`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_core::component::ComponentInfo;
use stepflow_core::workflow::{Component, StepId};
use stepflow_plugin::{DynPlugin, PluginError, Result, RunContext, StepflowEnvironment};
use tokio::sync::Mutex;

use stepflow_grpc::StepflowGrpcServer;
use stepflow_grpc::queue_plugin::StepflowQueuePlugin;

use crate::nats_transport::NatsTaskTransport;

pub use stepflow_config::NatsPluginConfig;

/// Factory for creating NATS JetStream transport plugins.
pub struct NatsPluginFactory;

impl stepflow_plugin::PluginFactory for NatsPluginFactory {
    type Config = NatsPluginConfig;

    /// Create a NATS JetStream transport plugin from configuration.
    async fn create_dyn(
        config: NatsPluginConfig,
        working_directory: &Path,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, PluginError> {
        if config.queue_timeout_secs == 0 {
            return Err(error_stack::report!(PluginError::Initializing)
                .attach_printable("queue_timeout_secs must be greater than 0"));
        }
        let consumer = config.consumer.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing).attach_printable(
                "consumer is required for NATS plugins — it identifies the \
                 durable consumer name for workers",
            )
        })?;

        let plugin = NatsPlugin {
            inner: Mutex::new(None),
            url: config.url,
            default_stream: config.stream,
            command: config.command,
            args: config.args,
            env: config.env,
            consumer,
            queue_timeout: std::time::Duration::from_secs(config.queue_timeout_secs),
            execution_timeout: config
                .execution_timeout_secs
                .map(std::time::Duration::from_secs),
            working_directory: working_directory.to_path_buf(),
            worker: Mutex::new(None),
        };

        Ok(DynPlugin::boxed(plugin))
    }
}

/// Runtime state for the worker subprocess (shared with GrpcPlugin).
struct WorkerState {
    #[allow(dead_code)]
    child: tokio::process::Child,
    #[cfg(unix)]
    pgid: i32,
}

impl Drop for WorkerState {
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.pgid > 0 {
            log::info!("Killing NATS worker process group {}", self.pgid);
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

/// NATS JetStream transport plugin.
///
/// Created by [`NatsPluginConfig`]. Dispatches tasks via NATS JetStream
/// and delegates task lifecycle tracking to [`StepflowQueuePlugin`].
pub struct NatsPlugin {
    inner: Mutex<Option<StepflowQueuePlugin>>,
    url: String,
    /// Default JetStream stream name (from plugin config).
    default_stream: Option<String>,
    consumer: String,
    command: Option<String>,
    args: Vec<String>,
    env: HashMap<String, String>,
    queue_timeout: std::time::Duration,
    execution_timeout: Option<std::time::Duration>,
    working_directory: PathBuf,
    worker: Mutex<Option<WorkerState>>,
}

impl NatsPlugin {
    /// Spawn a worker subprocess configured for NATS transport.
    async fn spawn_worker(&self, server_address: &str) -> Result<()> {
        let command = self.command.as_deref().ok_or_else(|| {
            error_stack::report!(PluginError::Execution)
                .attach_printable("Cannot spawn worker: no command configured")
        })?;

        // Drop old worker first
        {
            let mut worker = self.worker.lock().await;
            if let Some(old) = worker.take() {
                log::info!("Killing previous NATS worker subprocess");
                drop(old);
            }
        }

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(&self.args)
            .current_dir(&self.working_directory)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .env("STEPFLOW_TRANSPORT", "nats")
            .env("STEPFLOW_NATS_URL", &self.url)
            .env("STEPFLOW_NATS_CONSUMER", &self.consumer)
            // Workers still need the gRPC server for blob operations
            .env("STEPFLOW_BLOB_URL", server_address);

        // Pass the default stream if configured
        if let Some(ref stream) = self.default_stream {
            cmd.env("STEPFLOW_NATS_STREAM", stream);
        }

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
            .attach_printable_lazy(|| format!("Failed to spawn NATS worker: {command}"))?;

        #[cfg(unix)]
        let pgid = child.id().map(|pid| pid as i32).unwrap_or(0);

        log::info!(
            "NATS worker subprocess spawned (pid={:?}, queue={})",
            child.id(),
            self.consumer,
        );

        let worker_state = WorkerState {
            child,
            #[cfg(unix)]
            pgid,
        };

        // NATS workers are loosely coupled — we don't wait for registration.
        // The worker connects to NATS independently. Tasks are published to
        // the queue whether or not workers are listening; queue timeouts handle
        // the case where no worker picks up a task in time.
        *self.worker.lock().await = Some(worker_state);

        Ok(())
    }
}

impl stepflow_plugin::Plugin for NatsPlugin {
    async fn ensure_initialized(&self, env: &Arc<StepflowEnvironment>) -> Result<()> {
        // Idempotent
        {
            let inner = self.inner.lock().await;
            if inner.is_some() {
                return Ok(());
            }
        }

        // Connect to NATS and create the transport
        let transport = NatsTaskTransport::connect(&self.url, self.default_stream.clone()).await?;

        // The gRPC server is still needed for OrchestratorService
        // (heartbeats, completion, blobs, sub-runs).
        let shared_server = env.get::<Arc<StepflowGrpcServer>>().ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("StepflowGrpcServer not found in environment")
        })?;
        // Get the server address (set during startup via set_address).
        let server_address = shared_server.address().await.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("gRPC server address not set; ensure startup sets it before initializing NATS plugins")
        })?;

        let inner_plugin = StepflowQueuePlugin::new(
            Box::new(transport),
            shared_server.pending_tasks().clone(),
            self.queue_timeout,
            self.execution_timeout,
            self.consumer.clone(),
        );

        // Set the orchestrator URL for task assignments
        let advertised_address =
            std::env::var("STEPFLOW_ORCHESTRATOR_URL").unwrap_or(server_address.clone());
        inner_plugin.set_orchestrator_url(advertised_address);

        *self.inner.lock().await = Some(inner_plugin);

        // Spawn worker subprocess if command is configured
        if self.command.is_some() {
            self.spawn_worker(&server_address).await?;
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
        request: &stepflow_plugin::TaskRequest,
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
    ) -> Result<()> {
        let inner = self.inner.lock().await;
        let inner = inner.as_ref().ok_or_else(|| {
            error_stack::report!(PluginError::Execution).attach_printable("plugin not initialized")
        })?;
        inner.start_task(request, run_context, step).await
    }

    async fn prepare_for_retry(&self) -> Result<()> {
        // NATS workers are loosely coupled — no restart needed.
        // The connection is via NATS, not a direct process link.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stepflow_plugin::PluginFactory as _;

    /// Full config with all fields deserializes correctly.
    #[test]
    fn test_config_deserialization() {
        let json = serde_json::json!({
            "url": "nats://localhost:4222",
            "stream": "PYTHON_TASKS",
            "command": "python",
            "args": ["-m", "worker"],
            "env": {"FOO": "bar"},
            "consumer": "my-consumer",
            "queueTimeoutSecs": 60,
            "executionTimeoutSecs": 120
        });

        let config: NatsPluginConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.url, "nats://localhost:4222");
        assert_eq!(config.stream.as_deref(), Some("PYTHON_TASKS"));
        assert_eq!(config.command.as_deref(), Some("python"));
        assert_eq!(config.args, vec!["-m", "worker"]);
        assert_eq!(config.env.get("FOO").map(|s| s.as_str()), Some("bar"));
        assert_eq!(config.consumer.as_deref(), Some("my-consumer"));
        assert_eq!(config.queue_timeout_secs, 60);
        assert_eq!(config.execution_timeout_secs, Some(120));
    }

    /// Minimal config uses correct defaults for optional fields.
    #[test]
    fn test_config_defaults() {
        let json = serde_json::json!({
            "url": "nats://localhost:4222",
            "consumer": "my-consumer"
        });

        let config: NatsPluginConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.url, "nats://localhost:4222");
        assert!(config.stream.is_none());
        assert!(config.command.is_none());
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
        assert_eq!(
            config.queue_timeout_secs, 30,
            "Default should be 30 seconds"
        );
        assert!(config.execution_timeout_secs.is_none());
    }

    /// Config without `consumer` fails create with a descriptive error.
    #[tokio::test]
    async fn test_config_consumer_required() {
        let json = serde_json::json!({
            "url": "nats://localhost:4222"
        });

        let config: NatsPluginConfig = serde_json::from_value(json).unwrap();
        let result = NatsPluginFactory::create_dyn(config, Path::new("/tmp")).await;

        match result {
            Ok(_) => panic!("NatsPluginFactory::create should fail without consumer"),
            Err(err) => {
                let msg = format!("{err:?}");
                assert!(
                    msg.contains("consumer"),
                    "Error should mention 'consumer', got: {msg}"
                );
            }
        }
    }

    /// queueTimeoutSecs: 0 is rejected by NatsPluginFactory::create.
    #[tokio::test]
    async fn test_config_queue_timeout_zero_rejected() {
        let json = serde_json::json!({
            "url": "nats://localhost:4222",
            "consumer": "my-consumer",
            "queueTimeoutSecs": 0
        });

        let config: NatsPluginConfig = serde_json::from_value(json).unwrap();
        let result = NatsPluginFactory::create_dyn(config, Path::new("/tmp")).await;

        match result {
            Ok(_) => panic!("NatsPluginFactory::create should fail with queue_timeout_secs=0"),
            Err(err) => {
                let msg = format!("{err:?}");
                assert!(
                    msg.contains("queue_timeout_secs") || msg.contains("greater than 0"),
                    "Error should mention timeout constraint, got: {msg}"
                );
            }
        }
    }
}
