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

//! gRPC implementation of `TasksService`.
//!
//! This service runs on the orchestrator and is consumed by workers (workers
//! are *clients* of this service). It streams task assignments from a shared
//! [`PullTaskQueue`](crate::pull_task_queue::PullTaskQueue) to connected workers.
//!
//! Static worker configuration (blob service URL, blobification threshold,
//! etc.) is provided to workers via environment variables at deployment time,
//! not through this service. Only per-task information (orchestrator URL)
//! is sent in-band via `TaskContext`.
//!
//! Any orchestrator with access to the queue can serve this — it is a
//! stateless dispatcher.

use std::sync::Arc;

use stepflow_core::component::ComponentInfo;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::LeaseManagerExt as _;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::error as grpc_err;
use crate::grpc_server::QueueRegistry;
use crate::proto::stepflow::v1::tasks_service_server::TasksService;
use crate::proto::stepflow::v1::{
    GetOrchestratorForRunRequest, GetOrchestratorForRunResponse, PullTasksRequest, TaskAssignment,
};

/// gRPC implementation of `TasksService`.
///
/// Routes `PullTasks` requests to the correct [`PullTaskQueue`](crate::pull_task_queue::PullTaskQueue) based on
/// the worker's `queue_name`. Each pull plugin registers its queue under
/// a unique name in the shared [`QueueRegistry`].
///
/// Also provides `GetOrchestratorForRun` for workers to discover the current
/// orchestrator for a run after a failover or restart.
#[derive(Debug)]
pub struct TasksServiceImpl {
    registry: Arc<QueueRegistry>,
    env: Arc<StepflowEnvironment>,
}

impl TasksServiceImpl {
    pub fn new(registry: Arc<QueueRegistry>, env: Arc<StepflowEnvironment>) -> Self {
        Self { registry, env }
    }
}

#[tonic::async_trait]
impl TasksService for TasksServiceImpl {
    type PullTasksStream = ReceiverStream<Result<TaskAssignment, Status>>;

    async fn pull_tasks(
        &self,
        request: Request<PullTasksRequest>,
    ) -> Result<Response<Self::PullTasksStream>, Status> {
        let req = request.into_inner();

        if req.queue_name.is_empty() {
            return Err(grpc_err::invalid_field(
                "queue_name",
                "queue name is required",
            ));
        }

        // Look up the queue for this worker's queue_name
        let queue = self
            .registry
            .get(&req.queue_name)
            .ok_or_else(|| grpc_err::not_found("queue", &req.queue_name))?;

        // Convert proto ComponentInfo to domain ComponentInfo
        let components: Vec<ComponentInfo> = req
            .components
            .into_iter()
            .map(proto_component_to_domain)
            .collect();

        if components.is_empty() {
            return Err(grpc_err::invalid_field(
                "components",
                "at least one component must be declared",
            ));
        }

        let max_concurrent = if req.max_concurrent > 0 {
            req.max_concurrent as usize
        } else {
            1
        };

        let num_components = components.len();

        // Use the worker's self-assigned UUID if provided, otherwise fall
        // back to the internal connection counter for logging.
        let internal_id = queue.register_worker(components);
        let worker_label = if req.worker_id.is_empty() {
            format!("{internal_id}")
        } else {
            req.worker_id
        };

        log::info!(
            "Worker {} connected for queue '{}' with {} components, max_concurrent={}",
            worker_label,
            req.queue_name,
            num_components,
            max_concurrent,
        );

        // Create a channel for the response stream.
        // Buffer size matches max_concurrent so the worker can receive
        // multiple tasks without backpressure stalling the queue.
        let (tx, rx) = tokio::sync::mpsc::channel(max_concurrent);

        // Spawn a background task that pulls from the queue and sends
        // to the worker stream.
        tokio::spawn(async move {
            loop {
                // Try to pop a task immediately
                if let Some(task) = queue.pop_task() {
                    if tx.send(Ok(task)).await.is_err() {
                        // Worker disconnected
                        break;
                    }
                    continue;
                }

                // No task available — wait for notification or worker disconnect.
                // We use `tokio::select!` to handle both cases.
                tokio::select! {
                    _ = queue.notified() => {
                        // New task may be available, loop back to pop
                    }
                    _ = tx.closed() => {
                        // Worker disconnected
                        break;
                    }
                }
            }

            log::info!("Worker {} disconnected", worker_label);
            queue.unregister_worker(internal_id);
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_orchestrator_for_run(
        &self,
        request: Request<GetOrchestratorForRunRequest>,
    ) -> Result<Response<GetOrchestratorForRunResponse>, Status> {
        let req = request.into_inner();

        let run_id: uuid::Uuid = req
            .run_id
            .parse()
            .map_err(|_| grpc_err::invalid_field("run_id", "invalid UUID format"))?;

        let lease_manager = self.env.lease_manager();

        // Look up who owns this run
        let lease_info = lease_manager
            .get_lease(run_id)
            .await
            .map_err(|e| grpc_err::internal(format!("lease lookup failed: {e}")))?;

        let Some(lease) = lease_info else {
            return Err(grpc_err::not_found("run lease", &req.run_id));
        };

        // Find the owning orchestrator's service URL
        let orchestrators = lease_manager
            .list_orchestrators()
            .await
            .map_err(|e| grpc_err::internal(format!("orchestrator lookup failed: {e}")))?;

        let orchestrator_url = orchestrators
            .into_iter()
            .find(|o| o.id == lease.owner)
            .map(|o| o.orchestrator_url)
            .unwrap_or_default();

        if orchestrator_url.is_empty() {
            return Err(grpc_err::not_found(
                "orchestrator URL for owner",
                lease.owner.as_str(),
            ));
        }

        Ok(Response::new(GetOrchestratorForRunResponse {
            orchestrator_service_url: orchestrator_url,
        }))
    }
}

/// Convert a proto `ComponentInfo` to a domain `ComponentInfo`.
fn proto_component_to_domain(proto: crate::proto::stepflow::v1::ComponentInfo) -> ComponentInfo {
    ComponentInfo {
        component: stepflow_core::workflow::Component::from_string(proto.name),
        description: proto.description,
        input_schema: proto
            .input_schema
            .and_then(|s| serde_json::from_str(&s).ok()),
        output_schema: proto
            .output_schema
            .and_then(|s| serde_json::from_str(&s).ok()),
    }
}
