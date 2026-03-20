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

use tokio::sync::Notify;

use crate::proto::stepflow::v1::TaskAssignment;

/// Shared in-memory queue for pull-based task dispatch.
///
/// Thread-safe: uses `Mutex` for the queue and `Notify` for wakeups.
/// Component discovery is handled at the plugin level via
/// `ListComponentsRequest` tasks, not at connection time.
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
    /// Connected workers (for worker count and lifecycle tracking).
    workers: Mutex<Vec<WorkerRegistration>>,
    /// Monotonically increasing worker ID counter.
    next_worker_id: AtomicU64,
}

#[derive(Debug)]
struct WorkerRegistration {
    /// Unique ID for this worker connection.
    id: u64,
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
    /// Call `unregister_worker` when the connection closes.
    pub fn register_worker(&self) -> u64 {
        let mut workers = self.workers.lock().expect("lock poisoned");
        let id = self.next_worker_id.fetch_add(1, Ordering::Relaxed);
        workers.push(WorkerRegistration { id });
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

    fn make_task(id: &str) -> TaskAssignment {
        TaskAssignment {
            task_id: id.to_string(),
            task: None,
            context: None,
            deadline_secs: 0,
            heartbeat_interval_secs: 0,
            execution_timeout_secs: 0,
        }
    }

    #[test]
    fn test_push_pop() {
        let queue = PullTaskQueue::new("test");
        assert_eq!(queue.pending_count(), 0);

        queue.push_task(make_task("t1"));
        assert_eq!(queue.pending_count(), 1);

        let popped = queue.pop_task();
        assert!(popped.is_some());
        assert_eq!(popped.unwrap().task_id, "t1");
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn test_fifo_order() {
        let queue = PullTaskQueue::new("test");
        queue.push_task(make_task("t1"));
        queue.push_task(make_task("t2"));

        assert_eq!(queue.pop_task().unwrap().task_id, "t1");
        assert_eq!(queue.pop_task().unwrap().task_id, "t2");
    }

    #[test]
    fn test_worker_registration() {
        let queue = PullTaskQueue::new("test");
        assert_eq!(queue.worker_count(), 0);

        let id = queue.register_worker();
        assert_eq!(queue.worker_count(), 1);

        queue.unregister_worker(id);
        assert_eq!(queue.worker_count(), 0);
    }

    /// Two separate queues are fully independent — tasks pushed to one
    /// are never visible to the other.
    #[test]
    fn test_separate_queues_are_isolated() {
        let queue_a = PullTaskQueue::new("test");
        let queue_b = PullTaskQueue::new("test");

        queue_a.register_worker();
        queue_b.register_worker();

        queue_a.push_task(make_task("task-a1"));
        queue_a.push_task(make_task("task-a2"));
        queue_b.push_task(make_task("task-b1"));

        assert_eq!(queue_a.pending_count(), 2);
        assert_eq!(queue_b.pending_count(), 1);

        assert_eq!(queue_a.pop_task().unwrap().task_id, "task-a1");
        assert_eq!(queue_a.pop_task().unwrap().task_id, "task-a2");
        assert!(queue_a.pop_task().is_none());

        assert_eq!(queue_b.pop_task().unwrap().task_id, "task-b1");
        assert!(queue_b.pop_task().is_none());
    }

    /// Multiple workers on the same queue compete for tasks (work-stealing).
    #[tokio::test]
    async fn test_multiple_workers_compete_for_tasks() {
        let queue = std::sync::Arc::new(PullTaskQueue::new("test"));

        queue.register_worker();
        queue.register_worker();
        assert_eq!(queue.worker_count(), 2);

        for i in 0..4 {
            queue.push_task(make_task(&format!("task-{i}")));
        }

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

        let mut all_tasks: Vec<_> = tasks1.into_iter().chain(tasks2).collect();
        all_tasks.sort();
        assert_eq!(all_tasks, vec!["task-0", "task-1", "task-2", "task-3"]);
    }

    #[tokio::test]
    async fn test_notify_wakes_waiter() {
        let queue = std::sync::Arc::new(PullTaskQueue::new("test"));
        let queue2 = queue.clone();

        let handle = tokio::spawn(async move {
            queue2.notified().await;
            queue2.pop_task()
        });

        tokio::task::yield_now().await;

        queue.push_task(make_task("t1"));

        let result = handle.await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().task_id, "t1");
    }
}
