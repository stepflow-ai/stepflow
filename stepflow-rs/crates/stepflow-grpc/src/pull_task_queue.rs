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

//! Shared in-memory task queue for pull-based transport.
//!
//! [`PullTaskQueue`] is the shared state between `InMemoryTaskTransport`
//! (producer) and `TasksServiceImpl` (consumer). Tasks are pushed by the
//! transport and streamed to connected workers by the service.
//!
//! This is intentionally simple: all tasks go into a single FIFO queue and
//! any connected worker can pull from it. Component routing is handled at
//! a higher level by the plugin/routing system.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use stepflow_core::component::ComponentInfo;
use tokio::sync::Notify;

use crate::proto::stepflow::v1::TaskAssignment;

/// Shared in-memory queue for pull-based task dispatch.
///
/// Thread-safe: uses `Mutex` for the queue and `Notify` for wakeups.
/// Workers register their components on connect so `list_components`
/// can aggregate across all connected workers.
#[derive(Debug)]
pub struct PullTaskQueue {
    /// Human-readable name for this queue (used in metric labels).
    name: String,
    /// Pending tasks waiting to be assigned to a worker.
    tasks: Mutex<VecDeque<TaskAssignment>>,
    /// Notifies waiting workers when a new task arrives.
    notify: Notify,
    /// Notifies waiters when a new worker connects (for startup readiness).
    worker_notify: Notify,
    /// Components from connected workers (for `list_components`).
    workers: Mutex<Vec<WorkerRegistration>>,
    /// Monotonically increasing worker ID counter.
    next_worker_id: AtomicU64,
}

#[derive(Debug)]
struct WorkerRegistration {
    /// Unique ID for this worker connection.
    id: u64,
    /// Components this worker can handle.
    components: Vec<ComponentInfo>,
}

impl PullTaskQueue {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tasks: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            worker_notify: Notify::new(),
            workers: Default::default(),
            next_worker_id: AtomicU64::new(1),
        }
    }

    /// Push a task into the queue and notify waiting workers.
    pub fn push_task(&self, task: TaskAssignment) {
        {
            let mut queue = self.tasks.lock().expect("lock poisoned");
            queue.push_back(task);
        }
        stepflow_observability::metrics::record_task_dispatched(&self.name);
        stepflow_observability::metrics::record_queue_push(&self.name);
        self.notify.notify_waiters();
    }

    /// Try to pop the next task from the queue.
    pub fn pop_task(&self) -> Option<TaskAssignment> {
        let mut queue = self.tasks.lock().expect("lock poisoned");
        let task = queue.pop_front();
        if task.is_some() {
            stepflow_observability::metrics::record_queue_pop(&self.name);
        }
        task
    }

    /// Wait until notified that a new task may be available.
    pub async fn notified(&self) {
        self.notify.notified().await;
    }

    /// Register a worker and return its unique ID.
    ///
    /// The worker's components are added to the aggregate list used by
    /// `list_components`. Call `unregister_worker` when the connection closes.
    pub fn register_worker(&self, components: Vec<ComponentInfo>) -> u64 {
        let mut workers = self.workers.lock().expect("lock poisoned");
        let id = self.next_worker_id.fetch_add(1, Ordering::Relaxed);
        workers.push(WorkerRegistration { id, components });
        drop(workers);
        stepflow_observability::metrics::record_worker_connected(&self.name);
        self.worker_notify.notify_waiters();
        id
    }

    /// Wait until at least one worker is connected.
    ///
    /// Returns immediately if a worker is already connected. Uses a
    /// `Notify`-based wakeup so there is no polling overhead.
    pub async fn wait_for_worker(&self) {
        loop {
            if self.worker_count() > 0 {
                return;
            }
            self.worker_notify.notified().await;
        }
    }

    /// Wait until a worker with ID >= `since` has registered.
    ///
    /// Use [`Self::next_worker_generation`] to capture the current generation
    /// *before* spawning a subprocess, then pass that value here to avoid
    /// returning early because a stale (crashed) worker is still registered.
    pub async fn wait_for_worker_since(&self, since: u64) {
        loop {
            {
                let workers = self.workers.lock().expect("lock poisoned");
                if workers.iter().any(|w| w.id >= since) {
                    return;
                }
            }
            self.worker_notify.notified().await;
        }
    }

    /// Return the next worker ID that will be assigned.
    ///
    /// Capture this *before* spawning a worker subprocess and pass it to
    /// [`Self::wait_for_worker_since`] to wait specifically for the new worker.
    pub fn next_worker_generation(&self) -> u64 {
        self.next_worker_id.load(Ordering::Relaxed)
    }

    /// Unregister a worker when its connection closes.
    pub fn unregister_worker(&self, worker_id: u64) {
        let mut workers = self.workers.lock().expect("lock poisoned");
        let before = workers.len();
        workers.retain(|w| w.id != worker_id);
        if workers.len() < before {
            stepflow_observability::metrics::record_worker_disconnected(&self.name);
        }
    }

    /// Aggregate components from all connected workers.
    pub fn list_components(&self) -> Vec<ComponentInfo> {
        let workers = self.workers.lock().expect("lock poisoned");
        workers
            .iter()
            .flat_map(|w| w.components.iter().cloned())
            .collect()
    }

    /// Number of pending tasks in the queue.
    pub fn pending_count(&self) -> usize {
        let queue = self.tasks.lock().expect("lock poisoned");
        queue.len()
    }

    /// Number of connected workers.
    pub fn worker_count(&self) -> usize {
        let workers = self.workers.lock().expect("lock poisoned");
        workers.len()
    }
}

impl Default for PullTaskQueue {
    fn default() -> Self {
        Self::new("default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_component_info(name: &str) -> ComponentInfo {
        ComponentInfo {
            component: stepflow_core::workflow::Component::from_string(name.to_string()),
            description: None,
            input_schema: None,
            output_schema: None,
        }
    }

    #[test]
    fn test_push_pop() {
        let queue = PullTaskQueue::new("test");
        assert_eq!(queue.pending_count(), 0);

        let task = TaskAssignment {
            task_id: "t1".to_string(),
            request: None,
            deadline_secs: 0,
            heartbeat_interval_secs: 0,
            execution_timeout_secs: 0,
        };
        queue.push_task(task);
        assert_eq!(queue.pending_count(), 1);

        let popped = queue.pop_task();
        assert!(popped.is_some());
        assert_eq!(popped.unwrap().task_id, "t1");
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn test_fifo_order() {
        let queue = PullTaskQueue::new("test");
        queue.push_task(TaskAssignment {
            task_id: "t1".to_string(),
            request: None,
            deadline_secs: 0,
            heartbeat_interval_secs: 0,
            execution_timeout_secs: 0,
        });
        queue.push_task(TaskAssignment {
            task_id: "t2".to_string(),
            request: None,
            deadline_secs: 0,
            heartbeat_interval_secs: 0,
            execution_timeout_secs: 0,
        });

        assert_eq!(queue.pop_task().unwrap().task_id, "t1");
        assert_eq!(queue.pop_task().unwrap().task_id, "t2");
    }

    #[test]
    fn test_worker_registration() {
        let queue = PullTaskQueue::new("test");
        assert_eq!(queue.worker_count(), 0);

        let comps = vec![make_component_info("transform")];
        let id = queue.register_worker(comps);
        assert_eq!(queue.worker_count(), 1);

        let listed = queue.list_components();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].component.path(), "transform");

        queue.unregister_worker(id);
        assert_eq!(queue.worker_count(), 0);
        assert!(queue.list_components().is_empty());
    }

    #[test]
    fn test_multiple_workers_aggregate_components() {
        let queue = PullTaskQueue::new("test");
        let _id1 = queue.register_worker(vec![make_component_info("a")]);
        let _id2 = queue.register_worker(vec![make_component_info("b"), make_component_info("c")]);

        let listed = queue.list_components();
        assert_eq!(listed.len(), 3);
    }

    /// Two separate queues are fully independent — tasks pushed to one
    /// are never visible to the other. This models the per-plugin queue
    /// architecture where each `type: pull` plugin gets its own queue.
    #[test]
    fn test_separate_queues_are_isolated() {
        let queue_a = PullTaskQueue::new("test");
        let queue_b = PullTaskQueue::new("test");

        // Register different workers on each queue
        queue_a.register_worker(vec![make_component_info("python/transform")]);
        queue_b.register_worker(vec![make_component_info("node/summarize")]);

        // Push tasks to queue A only
        queue_a.push_task(TaskAssignment {
            task_id: "task-a1".to_string(),
            request: None,
            deadline_secs: 0,
            heartbeat_interval_secs: 0,
            execution_timeout_secs: 0,
        });
        queue_a.push_task(TaskAssignment {
            task_id: "task-a2".to_string(),
            request: None,
            deadline_secs: 0,
            heartbeat_interval_secs: 0,
            execution_timeout_secs: 0,
        });

        // Push tasks to queue B only
        queue_b.push_task(TaskAssignment {
            task_id: "task-b1".to_string(),
            request: None,
            deadline_secs: 0,
            heartbeat_interval_secs: 0,
            execution_timeout_secs: 0,
        });

        // Queue A has 2 tasks, queue B has 1
        assert_eq!(queue_a.pending_count(), 2);
        assert_eq!(queue_b.pending_count(), 1);

        // Popping from A only yields A's tasks
        assert_eq!(queue_a.pop_task().unwrap().task_id, "task-a1");
        assert_eq!(queue_a.pop_task().unwrap().task_id, "task-a2");
        assert!(queue_a.pop_task().is_none());

        // Popping from B only yields B's tasks
        assert_eq!(queue_b.pop_task().unwrap().task_id, "task-b1");
        assert!(queue_b.pop_task().is_none());

        // Components are isolated too
        assert_eq!(queue_a.list_components().len(), 1);
        assert_eq!(
            queue_a.list_components()[0].component.path(),
            "python/transform"
        );
        assert_eq!(queue_b.list_components().len(), 1);
        assert_eq!(
            queue_b.list_components()[0].component.path(),
            "node/summarize"
        );
    }

    /// Multiple workers on the same queue compete for tasks (work-stealing).
    #[tokio::test]
    async fn test_multiple_workers_compete_for_tasks() {
        let queue = std::sync::Arc::new(PullTaskQueue::new("test"));

        // Register two workers on the same queue
        queue.register_worker(vec![make_component_info("transform")]);
        queue.register_worker(vec![make_component_info("transform")]);
        assert_eq!(queue.worker_count(), 2);

        // Push 4 tasks
        for i in 0..4 {
            queue.push_task(TaskAssignment {
                task_id: format!("task-{i}"),
                request: None,
                deadline_secs: 0,
                heartbeat_interval_secs: 0,
                execution_timeout_secs: 0,
            });
        }

        // Simulate two "workers" racing to pop
        let q1 = queue.clone();
        let q2 = queue.clone();

        let handle1 = tokio::spawn(async move {
            let mut tasks = Vec::new();
            while let Some(task) = q1.pop_task() {
                tasks.push(task.task_id);
            }
            tasks
        });

        let handle2 = tokio::spawn(async move {
            let mut tasks = Vec::new();
            while let Some(task) = q2.pop_task() {
                tasks.push(task.task_id);
            }
            tasks
        });

        let tasks1 = handle1.await.unwrap();
        let tasks2 = handle2.await.unwrap();

        // Between the two workers, all 4 tasks were consumed exactly once
        let mut all_tasks: Vec<_> = tasks1.into_iter().chain(tasks2).collect();
        all_tasks.sort();
        assert_eq!(all_tasks, vec!["task-0", "task-1", "task-2", "task-3"]);
    }

    #[tokio::test]
    async fn test_notify_wakes_waiter() {
        let queue = std::sync::Arc::new(PullTaskQueue::new("test"));
        let queue2 = queue.clone();

        let handle = tokio::spawn(async move {
            // Wait for notification, then pop
            queue2.notified().await;
            queue2.pop_task()
        });

        // Give the waiter a moment to register
        tokio::task::yield_now().await;

        queue.push_task(TaskAssignment {
            task_id: "t1".to_string(),
            request: None,
            deadline_secs: 0,
            heartbeat_interval_secs: 0,
            execution_timeout_secs: 0,
        });

        let result = handle.await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().task_id, "t1");
    }
}
