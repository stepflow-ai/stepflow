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

//! In-memory task transport for gRPC pull-based worker dispatch.
//!
//! [`InMemoryTaskTransport`] implements [`TaskTransport`] by writing tasks
//! to [`PullTaskQueue`] instances managed by a shared [`QueueRegistry`].
//! Workers consume tasks via `TasksService::PullTasks`, which reads from
//! the same queues.
//!
//! Supports per-route `queueName` overrides: the `route_params` map can
//! contain a `queueName` key that selects which queue receives the task.
//! Falls back to the default queue name from the plugin config.

use std::sync::Arc;

use stepflow_plugin::Result;

use crate::grpc_server::QueueRegistry;
use crate::proto::stepflow::v1::TaskAssignment;
use crate::pull_task_queue::PullTaskQueue;
use crate::task_transport::TaskTransport;

/// In-memory transport backed by a [`QueueRegistry`].
///
/// Tasks are held in memory and served to workers via `TasksService.PullTasks`.
/// Supports multiple queues via route-level `queueName` overrides — a single
/// gRPC plugin can dispatch to different queues depending on the route.
///
/// New queues are created on-demand when a previously-unseen `queueName`
/// appears in route params, and automatically registered in the shared
/// [`QueueRegistry`] so that `TasksService` can serve them to workers.
pub struct InMemoryTaskTransport {
    /// Shared registry of all queues (also used by TasksServiceImpl).
    registry: Arc<QueueRegistry>,
    /// Default queue name from plugin config.
    default_queue_name: String,
}

impl InMemoryTaskTransport {
    /// Create a new in-memory transport.
    ///
    /// The `default_queue_name` is used when route params don't specify
    /// a `queueName`. A queue with this name is pre-registered in the
    /// registry.
    pub fn new(registry: Arc<QueueRegistry>, default_queue_name: String) -> Self {
        // Ensure the default queue exists in the registry.
        registry.get_or_create(&default_queue_name);
        Self {
            registry,
            default_queue_name,
        }
    }

    /// Get the default queue (for backward compatibility with code that
    /// needs a direct reference, e.g., `wait_for_worker_since`).
    pub fn default_queue(&self) -> Arc<PullTaskQueue> {
        self.registry
            .get(&self.default_queue_name)
            .expect("default queue must exist")
    }
}

#[tonic::async_trait]
impl TaskTransport for InMemoryTaskTransport {
    async fn send_task(
        &self,
        task: TaskAssignment,
        route_params: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        // Resolve queue name: route param overrides plugin default
        let queue_name = route_params
            .get("queueName")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.default_queue_name);

        let queue = self.registry.get_or_create(queue_name);
        queue.push_task(task);
        Ok(())
    }
}

#[tonic::async_trait]
impl crate::task_transport::TaskTransportRead for InMemoryTaskTransport {
    async fn recv_task(
        &self,
        subject_or_queue: &str,
        timeout: std::time::Duration,
    ) -> Result<Option<TaskAssignment>> {
        let queue = match self.registry.get(subject_or_queue) {
            Some(q) => q,
            None => return Ok(None),
        };

        // Poll the queue with a timeout
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(task) = queue.pop_task() {
                return Ok(Some(task));
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::transport_compliance::TransportComplianceTests;

    fn make_task(id: &str) -> TaskAssignment {
        TaskAssignment {
            task_id: id.to_string(),
            task: None,
            context: None,
            heartbeat_interval_secs: 1,
        }
    }

    /// Run the full transport compliance suite (including round-trip tests)
    /// against InMemoryTaskTransport.
    #[tokio::test]
    async fn compliance_all() {
        use crate::task_transport::TaskTransportRead;

        let counter = std::sync::atomic::AtomicUsize::new(0);
        TransportComplianceTests::run_all_readable(
            || {
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    let registry = Arc::new(QueueRegistry::default());
                    let default_q = format!("test-{n}");
                    let alt_q = format!("test-{n}-alt");
                    let transport =
                        Box::new(InMemoryTaskTransport::new(registry, default_q.clone()))
                            as Box<dyn TaskTransportRead>;
                    (transport, default_q, alt_q)
                }
            },
            "queueName",
        )
        .await;
    }

    /// Sending a task with a `queueName` route param routes it to
    /// the specified queue instead of the default.
    #[tokio::test]
    async fn test_route_param_queue_name_override() {
        let registry = Arc::new(QueueRegistry::default());
        let transport = InMemoryTaskTransport::new(registry.clone(), "default".to_string());

        let mut params = std::collections::HashMap::new();
        params.insert(
            "queueName".to_string(),
            serde_json::Value::String("other".to_string()),
        );

        transport
            .send_task(make_task("t1"), &params)
            .await
            .expect("send_task should succeed");

        // The default queue should be empty
        let default_queue = registry.get("default").expect("default queue must exist");
        assert!(
            default_queue.pop_task().is_none(),
            "Default queue should have no tasks"
        );

        // The "other" queue should have the task
        let other_queue = registry.get("other").expect("other queue must exist");
        let task = other_queue
            .pop_task()
            .expect("other queue should have a task");
        assert_eq!(task.task_id, "t1");
    }

    /// Without route params, send_task uses the default queue name.
    #[tokio::test]
    async fn test_default_queue_name_used() {
        let registry = Arc::new(QueueRegistry::default());
        let transport = InMemoryTaskTransport::new(registry.clone(), "my-default".to_string());

        let params = std::collections::HashMap::new();
        transport
            .send_task(make_task("t1"), &params)
            .await
            .expect("send_task should succeed");

        let queue = registry
            .get("my-default")
            .expect("default queue must exist");
        let task = queue.pop_task().expect("default queue should have a task");
        assert_eq!(task.task_id, "t1");
    }
}
