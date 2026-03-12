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

//! gRPC server for worker-facing services.
//!
//! [`StepflowGrpcServer`] is created once by the orchestrator and hosts all
//! worker-facing gRPC services:
//!
//! - [`TasksService`] — streams task assignments to workers (multi-queue)
//! - [`OrchestratorService`] — handles task completion, sub-runs, heartbeats
//! - [`BlobService`] — blob storage operations
//!
//! The server is stored in the [`StepflowEnvironment`] and started lazily
//! on first use. Plugins register their task queues via [`register_queue`].
//!
//! [`PullPlugin`]: crate::grpc_plugin_config::PullPlugin
//! [`register_queue`]: StepflowGrpcServer::register_queue

use std::sync::Arc;

use dashmap::DashMap;
use error_stack::ResultExt as _;
use stepflow_plugin::{PluginError, StepflowEnvironment};
use tokio::sync::Mutex;

use crate::pending_tasks::PendingTasks;
use crate::proto::stepflow::v1::blob_service_server::BlobServiceServer;
use crate::proto::stepflow::v1::orchestrator_service_server::OrchestratorServiceServer;
use crate::proto::stepflow::v1::tasks_service_server::TasksServiceServer;
use crate::pull_task_queue::PullTaskQueue;
use crate::services::{BlobServiceImpl, OrchestratorServiceImpl, TasksServiceImpl};

/// Registry mapping queue names to their [`PullTaskQueue`] instances.
///
/// Used by [`TasksServiceImpl`] to route `PullTasks` requests to the
/// correct queue based on the worker's `queue_name`.
#[derive(Debug, Default)]
pub struct QueueRegistry {
    queues: DashMap<String, Arc<PullTaskQueue>>,
}

impl QueueRegistry {
    /// Register a queue under the given name.
    pub fn register(&self, name: String, queue: Arc<PullTaskQueue>) {
        self.queues.insert(name, queue);
    }

    /// Look up a queue by name.
    pub fn get(&self, name: &str) -> Option<Arc<PullTaskQueue>> {
        self.queues.get(name).map(|r| r.value().clone())
    }
}

/// gRPC server for all worker-facing services.
///
/// Created once and stored in the [`StepflowEnvironment`]. Plugins register
/// their task queues via [`register_queue`] and workers connect to the
/// single server address.
///
/// [`register_queue`]: StepflowGrpcServer::register_queue
pub struct StepflowGrpcServer {
    /// Registry of queue_name → PullTaskQueue, shared with TasksServiceImpl.
    queue_registry: Arc<QueueRegistry>,
    /// Shared task completion registry, shared with OrchestratorServiceImpl.
    pending_tasks: Arc<PendingTasks>,
    /// Server state: None before start, Some after.
    state: Mutex<Option<ServerState>>,
}

struct ServerState {
    /// Address the server is listening on (e.g., "127.0.0.1:12345").
    address: String,
    /// Server task handle — aborted on drop.
    server_handle: tokio::task::JoinHandle<()>,
}

impl Drop for StepflowGrpcServer {
    fn drop(&mut self) {
        if let Some(state) = self.state.get_mut().take() {
            state.server_handle.abort();
        }
    }
}

impl std::fmt::Debug for StepflowGrpcServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepflowGrpcServer")
            .field("queue_registry", &self.queue_registry)
            .finish()
    }
}

impl Default for StepflowGrpcServer {
    fn default() -> Self {
        Self::new()
    }
}

impl StepflowGrpcServer {
    /// Create a new server (not yet started).
    ///
    /// Call [`ensure_started`] to bind and start the gRPC server.
    ///
    /// [`ensure_started`]: StepflowGrpcServer::ensure_started
    pub fn new() -> Self {
        Self {
            queue_registry: Arc::new(QueueRegistry::default()),
            pending_tasks: PendingTasks::new(),
            state: Mutex::new(None),
        }
    }

    /// Register a task queue under the given name.
    ///
    /// Workers specify this name in `PullTasksRequest.queue_name` to receive
    /// tasks from the corresponding queue.
    pub fn register_queue(&self, name: String, queue: Arc<PullTaskQueue>) {
        self.queue_registry.register(name, queue);
    }

    /// Get the shared pending tasks registry.
    ///
    /// Used by [`StepflowQueuePlugin`] to register task waiters.
    ///
    /// [`StepflowQueuePlugin`]: crate::queue_plugin::StepflowQueuePlugin
    pub fn pending_tasks(&self) -> &Arc<PendingTasks> {
        &self.pending_tasks
    }

    /// Get the server address (e.g., "127.0.0.1:12345").
    ///
    /// Returns `None` if the server has not been started yet.
    pub async fn address(&self) -> Option<String> {
        self.state
            .lock()
            .await
            .as_ref()
            .map(|s| s.address.clone())
    }

    /// Start the gRPC server if not already running. Idempotent.
    ///
    /// Binds to `127.0.0.1:0` (random port), starts the server in a
    /// background task, and returns the bound address.
    pub async fn ensure_started(
        &self,
        env: &Arc<StepflowEnvironment>,
    ) -> stepflow_plugin::Result<String> {
        let mut state = self.state.lock().await;
        if let Some(ref s) = *state {
            return Ok(s.address.clone());
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .change_context(PluginError::Initializing)
            .attach_printable("Failed to bind gRPC server")?;
        let port = listener
            .local_addr()
            .change_context(PluginError::Initializing)?
            .port();
        let address = format!("127.0.0.1:{port}");

        let orchestrator_service =
            OrchestratorServiceImpl::new(env.clone(), self.pending_tasks.clone());
        let tasks_service = TasksServiceImpl::new(self.queue_registry.clone());
        let blob_service = BlobServiceImpl::new(env.clone());

        let server_handle = tokio::spawn(async move {
            let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
            if let Err(e) = tonic::transport::Server::builder()
                .add_service(OrchestratorServiceServer::new(orchestrator_service))
                .add_service(TasksServiceServer::new(tasks_service))
                .add_service(BlobServiceServer::new(blob_service))
                .serve_with_incoming(incoming)
                .await
            {
                log::error!("gRPC server error: {e}");
            }
        });

        log::info!("gRPC server started on {address}");

        let addr = address.clone();
        *state = Some(ServerState {
            address,
            server_handle,
        });

        Ok(addr)
    }
}
