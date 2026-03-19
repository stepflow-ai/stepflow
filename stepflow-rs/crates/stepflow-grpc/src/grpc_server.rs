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

//! gRPC server for Stepflow services.
//!
//! [`StepflowGrpcServer`] is created once by the orchestrator and hosts all
//! gRPC services — both worker-facing and client-facing:
//!
//! **Worker-facing:**
//! - `TasksService` — streams task assignments to workers (multi-queue)
//! - `OrchestratorService` — handles task completion, sub-runs, heartbeats
//!
//! **Client-facing:**
//! - `FlowsService` — flow storage and retrieval
//! - `RunsService` — run creation, listing, and event streaming
//! - `HealthService` — health checks
//! - `ComponentsService` — component discovery
//! - `BlobService` — blob storage operations (shared)
//!
//! The server is stored in the [`StepflowEnvironment`] and its address is set
//! during service startup. Plugins register their task queues via
//! [`register_queue`] and retrieve the server address via [`address`].
//!
//! [`PullPlugin`]: crate::grpc_plugin_config::PullPlugin
//! [`register_queue`]: StepflowGrpcServer::register_queue
//! [`address`]: StepflowGrpcServer::address

use std::sync::Arc;

use dashmap::DashMap;
use stepflow_plugin::StepflowEnvironment;
use tokio::sync::Mutex;

use crate::pending_tasks::PendingTasks;
use crate::proto::stepflow::v1::blob_service_server::BlobServiceServer;
use crate::proto::stepflow::v1::components_service_server::ComponentsServiceServer;
use crate::proto::stepflow::v1::flows_service_server::FlowsServiceServer;
use crate::proto::stepflow::v1::health_service_server::HealthServiceServer;
use crate::proto::stepflow::v1::orchestrator_service_server::OrchestratorServiceServer;
use crate::proto::stepflow::v1::runs_service_server::RunsServiceServer;
use crate::proto::stepflow::v1::tasks_service_server::TasksServiceServer;
use crate::pull_task_queue::PullTaskQueue;
use crate::services::{
    BlobServiceImpl, ComponentsServiceImpl, FlowsServiceImpl, HealthServiceImpl,
    OrchestratorServiceImpl, RunsServiceImpl, TasksServiceImpl,
};

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

/// gRPC server for all Stepflow services (worker-facing and client-facing).
///
/// Created once and stored in the [`StepflowEnvironment`]. The server address
/// is set by [`StepflowService`] during startup via [`set_address`]. Plugins
/// register their task queues via [`register_queue`] and workers connect to
/// the single server address.
///
/// [`StepflowService`]: stepflow_server::StepflowService
/// [`set_address`]: StepflowGrpcServer::set_address
/// [`register_queue`]: StepflowGrpcServer::register_queue
pub struct StepflowGrpcServer {
    /// Registry of queue_name → PullTaskQueue, shared with TasksServiceImpl.
    queue_registry: Arc<QueueRegistry>,
    /// Shared task completion registry, shared with OrchestratorServiceImpl.
    pending_tasks: Arc<PendingTasks>,
    /// Server address, set once during startup.
    address: Mutex<Option<String>>,
}

impl std::fmt::Debug for StepflowGrpcServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepflowGrpcServer")
            .field("queue_registry", &self.queue_registry)
            .finish()
    }
}

impl StepflowGrpcServer {
    /// Create a new server (address not yet set).
    ///
    /// The `task_registry` is the shared in-memory registry for result
    /// delivery. It is passed to [`PendingTasks`] which adds gRPC-specific
    /// timeout and heartbeat tracking on top.
    pub fn new(task_registry: Arc<stepflow_plugin::TaskRegistry>) -> Self {
        Self {
            queue_registry: Arc::new(QueueRegistry::default()),
            pending_tasks: PendingTasks::new(task_registry),
            address: Mutex::new(None),
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

    /// Set the address where gRPC services are reachable.
    ///
    /// Called by [`StepflowService`] during startup after the port is bound
    /// and gRPC routes are multiplexed on the HTTP server. Pull plugins use
    /// this address to tell workers where to connect.
    ///
    /// [`StepflowService`]: stepflow_server::StepflowService
    pub async fn set_address(&self, addr: String) {
        let mut address = self.address.lock().await;
        if address.is_some() {
            log::warn!("gRPC server address already set; overwriting");
        }
        log::info!("gRPC services available on {addr}");
        *address = Some(addr);
    }

    /// Get the server address (e.g., `"127.0.0.1:7840"`).
    ///
    /// Returns `None` if [`set_address`](Self::set_address) has not been
    /// called yet.
    pub async fn address(&self) -> Option<String> {
        self.address.lock().await.clone()
    }

    /// Build gRPC service routes as an [`axum::Router`] for multiplexing
    /// with the HTTP server on a single port.
    ///
    /// The returned router handles all `application/grpc` requests via
    /// tonic's path-based routing (`/{service_name}/{method}`).
    pub fn build_grpc_router(&self, env: &Arc<StepflowEnvironment>) -> axum::Router {
        // Worker-facing services
        let orchestrator_service =
            OrchestratorServiceImpl::new(env.clone(), self.pending_tasks.clone());
        let tasks_service = TasksServiceImpl::new(self.queue_registry.clone(), env.clone());

        // Client-facing services
        let blob_service = BlobServiceImpl::new(env.clone());
        let flows_service = FlowsServiceImpl::new(env.clone());
        let runs_service = RunsServiceImpl::new(env.clone());
        let health_service = HealthServiceImpl::new();
        let components_service = ComponentsServiceImpl::new(env.clone());

        tonic::service::Routes::new(OrchestratorServiceServer::new(orchestrator_service))
            .add_service(TasksServiceServer::new(tasks_service))
            .add_service(BlobServiceServer::new(blob_service))
            .add_service(FlowsServiceServer::new(flows_service))
            .add_service(RunsServiceServer::new(runs_service))
            .add_service(HealthServiceServer::new(health_service))
            .add_service(ComponentsServiceServer::new(components_service))
            .prepare()
            .into_axum_router()
    }

    /// Start a standalone gRPC server for testing.
    ///
    /// Production code should use [`StepflowService`] which multiplexes gRPC
    /// on the HTTP port instead. This method is provided for integration tests
    /// that need a real gRPC server without the full HTTP stack.
    ///
    /// [`StepflowService`]: stepflow_server::StepflowService
    pub async fn start_standalone(
        &self,
        env: &Arc<StepflowEnvironment>,
    ) -> stepflow_plugin::Result<String> {
        use error_stack::ResultExt as _;
        use stepflow_plugin::PluginError;

        let bind_addr = std::env::var("STEPFLOW_BIND_ADDRESS")
            .unwrap_or_else(|_| "127.0.0.1:0".to_string());

        let listener = tokio::net::TcpListener::bind(&bind_addr)
            .await
            .change_context(PluginError::Initializing)
            .attach_printable_lazy(|| format!("Failed to bind gRPC server to {bind_addr}"))?;
        let address = listener
            .local_addr()
            .change_context(PluginError::Initializing)?
            .to_string();

        // Worker-facing services
        let orchestrator_service =
            OrchestratorServiceImpl::new(env.clone(), self.pending_tasks.clone());
        let tasks_service = TasksServiceImpl::new(self.queue_registry.clone(), env.clone());

        // Client-facing services
        let blob_service = BlobServiceImpl::new(env.clone());
        let flows_service = FlowsServiceImpl::new(env.clone());
        let runs_service = RunsServiceImpl::new(env.clone());
        let health_service = HealthServiceImpl::new();
        let components_service = ComponentsServiceImpl::new(env.clone());

        tokio::spawn(async move {
            let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
            if let Err(e) = tonic::transport::Server::builder()
                .add_service(OrchestratorServiceServer::new(orchestrator_service))
                .add_service(TasksServiceServer::new(tasks_service))
                .add_service(BlobServiceServer::new(blob_service))
                .add_service(FlowsServiceServer::new(flows_service))
                .add_service(RunsServiceServer::new(runs_service))
                .add_service(HealthServiceServer::new(health_service))
                .add_service(ComponentsServiceServer::new(components_service))
                .serve_with_incoming(incoming)
                .await
            {
                log::error!("gRPC server error: {e}");
            }
        });

        log::info!("Standalone gRPC server started on {address}");
        self.set_address(address.clone()).await;

        Ok(address)
    }
}
