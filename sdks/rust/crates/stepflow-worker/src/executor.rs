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

//! Task executor abstraction.
//!
//! The [`TaskExecutor`] trait decouples *how* tasks are handled from the
//! transport loop (gRPC / NATS) in [`Worker`](crate::Worker). This enables
//! different execution strategies:
//!
//! - [`LocalExecutor`] — runs components locally via [`ComponentRegistry`].
//! - Custom executors (e.g., isolation proxy) — forward tasks to sandboxed workers.

use std::sync::Arc;

use async_trait::async_trait;
use tonic::transport::Channel;

use stepflow_proto::TaskAssignment;

use crate::task_handler::{
    ChannelCache, handle_task, make_lazy_channel, new_channel_cache, normalize_url,
};
use crate::worker::WorkerConfig;
use crate::{ComponentRegistry, WorkerError};

/// Abstraction over how a worker handles task assignments.
///
/// The [`Worker`](crate::Worker) transport loop calls
/// [`execute_task`](TaskExecutor::execute_task) for every assignment (both
/// `Execute` and `ListComponents`). The transport loop owns concurrency
/// control, reconnection, and graceful shutdown.
///
/// # Implementations
///
/// - [`LocalExecutor`] — executes components locally via [`ComponentRegistry`].
///   This is the default executor created by [`Worker::new`](crate::Worker::new).
/// - Custom executors can forward tasks to sandboxed workers (VMs, containers, etc.)
///   by implementing this trait and passing it to
///   [`Worker::with_executor`](crate::Worker::with_executor).
#[async_trait]
pub trait TaskExecutor: Send + Sync + 'static {
    /// Handle a single task assignment to completion.
    ///
    /// Called for all task types (Execute, ListComponents, etc.). The executor
    /// is responsible for the full lifecycle: claiming the task (initial
    /// heartbeat), executing or forwarding, and sending `CompleteTask` to
    /// the orchestrator.
    ///
    /// For forwarding executors (e.g., the isolation proxy), the sandboxed
    /// worker handles the lifecycle directly — the executor just dispatches
    /// the assignment and waits for the sandbox to finish.
    ///
    /// This method should not panic. Errors should be reported to the
    /// orchestrator as task errors, not propagated to the caller.
    async fn execute_task(&self, assignment: TaskAssignment);
}

/// Executor that runs components locally via the [`ComponentRegistry`].
///
/// This is the default executor used by [`Worker::new`](crate::Worker::new).
/// It captures all configuration parameters at construction time so that
/// [`execute_task`](TaskExecutor::execute_task) only needs the `TaskAssignment`.
pub struct LocalExecutor {
    registry: Arc<ComponentRegistry>,
    worker_id: String,
    orchestrator_url: String,
    default_normalised_url: String,
    blob_url: Option<String>,
    blob_threshold_bytes: usize,
    default_orch_channel: Channel,
    channel_cache: ChannelCache,
}

impl LocalExecutor {
    /// Create a new `LocalExecutor`.
    ///
    /// The executor creates a lazily-connecting gRPC channel for the orchestrator
    /// URL derived from `config`.
    pub fn new(
        registry: Arc<ComponentRegistry>,
        config: &WorkerConfig,
        worker_id: String,
    ) -> Result<Self, WorkerError> {
        let orchestrator_url = config
            .orchestrator_url
            .clone()
            .unwrap_or_else(|| config.tasks_url.clone());
        let default_normalised_url = normalize_url(&orchestrator_url);
        let default_orch_channel = make_lazy_channel(&orchestrator_url).ok_or_else(|| {
            WorkerError::Config(format!("Invalid orchestrator URL: {orchestrator_url}"))
        })?;

        Ok(Self {
            registry,
            worker_id,
            orchestrator_url,
            default_normalised_url,
            blob_url: config.blob_url.clone(),
            blob_threshold_bytes: config.blob_threshold_bytes,
            default_orch_channel,
            channel_cache: new_channel_cache(),
        })
    }
}

#[async_trait]
impl TaskExecutor for LocalExecutor {
    async fn execute_task(&self, assignment: TaskAssignment) {
        handle_task(
            assignment,
            Arc::clone(&self.registry),
            &self.worker_id,
            &self.orchestrator_url,
            &self.default_normalised_url,
            self.blob_url.as_deref(),
            self.blob_threshold_bytes,
            self.default_orch_channel.clone(),
            &self.channel_cache,
        )
        .await;
    }
}
