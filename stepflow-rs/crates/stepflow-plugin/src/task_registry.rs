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

//! In-memory registry for tracking in-flight tasks and delivering results.
//!
//! [`TaskRegistry`] decouples task dispatch from result collection: the executor
//! registers a task and awaits its result via a oneshot channel, while plugins
//! (or external workers via gRPC) deliver results by calling [`complete`].
//!
//! This is the shared result-delivery mechanism used by all plugin types.
//! Queue-based transports (gRPC pull) add timeout and heartbeat tracking on top
//! via their own data structures, but route result delivery through this registry.
//!
//! [`complete`]: TaskRegistry::complete

use std::sync::Arc;

use dashmap::DashMap;
use stepflow_core::{FlowResult, StepflowEnvironment};
use tokio::sync::oneshot;

/// In-memory registry for tracking in-flight tasks and delivering results.
///
/// Each task is identified by a string task_id (typically a UUID). The registry
/// maps task_ids to oneshot senders. When a result arrives (via plugin completion
/// or gRPC CompleteTask RPC), the sender delivers the result to the awaiting
/// executor.
pub struct TaskRegistry {
    tasks: DashMap<String, oneshot::Sender<FlowResult>>,
}

impl TaskRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            tasks: DashMap::new(),
        }
    }

    /// Register a task and return a receiver for its result.
    ///
    /// The caller (executor) awaits the receiver. The result is delivered when
    /// [`complete`] is called with the same task_id.
    ///
    /// # Arguments
    /// * `task_id` - Unique identifier for the task
    ///
    /// [`complete`]: TaskRegistry::complete
    pub fn register(&self, task_id: String) -> oneshot::Receiver<FlowResult> {
        let (tx, rx) = oneshot::channel();
        self.tasks.insert(task_id, tx);
        rx
    }

    /// Complete a task by sending its result to the awaiting executor.
    ///
    /// Returns `true` if the task was found and the result delivered,
    /// `false` if the task_id was not found (already completed, timed out,
    /// or removed).
    pub fn complete(&self, task_id: &str, result: FlowResult) -> bool {
        let Some((_, sender)) = self.tasks.remove(task_id) else {
            return false;
        };
        // If the receiver was dropped (executor cancelled), the send fails
        // silently. This is fine — the task is cleaned up either way.
        let _ = sender.send(result);
        true
    }

    /// Remove a task registration without completing it.
    ///
    /// The awaiting receiver will get a `RecvError` (sender dropped).
    /// Used when the executor decides to cancel or clean up a task
    /// (e.g., safety timeout, start_task failure).
    pub fn remove(&self, task_id: &str) {
        self.tasks.remove(task_id);
    }

    /// Check whether a task is currently registered.
    pub fn contains(&self, task_id: &str) -> bool {
        self.tasks.contains_key(task_id)
    }

    /// Number of currently registered tasks.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension trait providing [`TaskRegistry`] access on [`StepflowEnvironment`].
pub trait TaskRegistryExt {
    /// Get the shared task registry.
    ///
    /// # Panics
    ///
    /// Panics if the task registry was not set during environment construction.
    fn task_registry(&self) -> Arc<TaskRegistry>;
}

impl TaskRegistryExt for StepflowEnvironment {
    fn task_registry(&self) -> Arc<TaskRegistry> {
        self.get::<Arc<TaskRegistry>>()
            .expect("TaskRegistry not set in environment")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stepflow_core::workflow::ValueRef;

    #[tokio::test]
    async fn register_and_complete() {
        let registry = TaskRegistry::new();
        let rx = registry.register("task-1".to_string());

        assert!(registry.contains("task-1"));
        assert_eq!(registry.len(), 1);

        let value = ValueRef::new(serde_json::json!({"result": 42}));
        assert!(registry.complete("task-1", FlowResult::Success(value.clone())));

        let result = rx.await.unwrap();
        match result {
            FlowResult::Success(v) => assert_eq!(v, value),
            _ => panic!("expected Success"),
        }

        assert!(!registry.contains("task-1"));
        assert!(registry.is_empty());
    }

    #[test]
    fn complete_unknown_returns_false() {
        let registry = TaskRegistry::new();
        let value = ValueRef::new(serde_json::json!(null));
        assert!(!registry.complete("unknown", FlowResult::Success(value)));
    }

    #[test]
    fn remove_drops_sender() {
        let registry = TaskRegistry::new();
        let mut rx = registry.register("task-2".to_string());
        registry.remove("task-2");

        assert!(!registry.contains("task-2"));
        // Receiver should get an error since sender was dropped
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn complete_after_receiver_dropped() {
        let registry = TaskRegistry::new();
        let rx = registry.register("task-3".to_string());
        drop(rx);

        // complete should still return true (task was found), even though
        // the receiver was dropped — the send silently fails.
        let value = ValueRef::new(serde_json::json!(null));
        assert!(registry.complete("task-3", FlowResult::Success(value)));
    }
}
