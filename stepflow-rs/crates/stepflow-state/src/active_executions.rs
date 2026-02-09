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

//! Active execution tracker for managing running workflow executions.
//!
//! This module provides [`ActiveExecutions`], a concurrent map that tracks all
//! running workflow executions by their root run ID. This enables:
//!
//! - Graceful shutdown with lease release and cancellation
//! - Monitoring the number of active executions
//! - Preventing duplicate execution of the same run
//!
//! # Example
//!
//! ```ignore
//! let active = ActiveExecutions::new();
//!
//! // Spawn and track an executor - cleanup is automatic
//! flow_executor.spawn(env.active_executions());
//!
//! // Check how many are running
//! println!("Active executions: {}", active.count());
//!
//! // Graceful shutdown
//! active.shutdown();
//! ```

use std::future::Future;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Tracks active workflow executions.
///
/// Each execution is tracked by its root run ID. When spawned via [`spawn`](Self::spawn),
/// cleanup is automatic when the execution completes.
///
/// This type is cheaply cloneable (wraps an Arc internally).
pub struct ActiveExecutions {
    /// Map of root_run_id -> task handle
    executions: Arc<DashMap<Uuid, JoinHandle<()>>>,
}

impl Default for ActiveExecutions {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ActiveExecutions {
    fn clone(&self) -> Self {
        Self {
            executions: self.executions.clone(),
        }
    }
}

impl ActiveExecutions {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            executions: Arc::new(DashMap::new()),
        }
    }

    /// Spawn and track an execution by its root run ID.
    ///
    /// The execution is spawned as a tokio task and automatically removed from
    /// tracking when it completes.
    ///
    /// # Arguments
    /// * `root_run_id` - The root run ID to track
    /// * `future` - The future to spawn
    pub fn spawn<F>(&self, root_run_id: Uuid, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let executions = self.executions.clone();
        let handle = tokio::spawn(async move {
            future.await;
            // No .await after remove() - task completes atomically after remove returns,
            // preventing any gap between cleanup and is_finished() becoming true.
            executions.remove(&root_run_id);
        });
        // Hold entry lock to prevent race with task's remove().
        // If task finished before we get here, is_finished() is true and we skip insert.
        // If task is still running, its remove() blocks on this lock until we release.
        let entry = self.executions.entry(root_run_id);
        if !handle.is_finished() {
            entry.insert(handle);
        }
    }

    /// Check if a run is currently being executed.
    pub fn contains(&self, root_run_id: &Uuid) -> bool {
        self.executions.contains_key(root_run_id)
    }

    /// Get the number of active executions.
    pub fn count(&self) -> usize {
        self.executions.len()
    }

    /// Check if there are no active executions.
    pub fn is_empty(&self) -> bool {
        self.executions.is_empty()
    }

    /// Initiate graceful shutdown.
    ///
    /// This aborts all running execution tasks. Note that this does not
    /// release leases - that should be handled by the caller if needed.
    ///
    /// # Returns
    /// The number of executions that were cancelled.
    pub fn shutdown(&self) -> usize {
        let count = self.executions.len();

        // Abort all running tasks
        for entry in self.executions.iter() {
            entry.value().abort();
        }

        // Clear the map
        self.executions.clear();

        log::info!("Shutdown: cancelled {} active executions", count);
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let active = ActiveExecutions::new();
        assert!(active.is_empty());
        assert_eq!(active.count(), 0);
    }

    #[tokio::test]
    async fn test_spawn_and_auto_cleanup() {
        let active = ActiveExecutions::new();
        let run_id = Uuid::now_v7();

        // Spawn a task that completes immediately
        active.spawn(run_id, async {});

        // Give the task time to complete and clean up
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Should be automatically removed after completion
        assert!(!active.contains(&run_id));
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn test_shutdown() {
        let active = ActiveExecutions::new();
        let run_id = Uuid::now_v7();

        // Spawn a long-running task
        active.spawn(run_id, async {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        });

        assert_eq!(active.count(), 1);

        // Shutdown should abort and clear
        let cancelled = active.shutdown();
        assert_eq!(cancelled, 1);
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn test_clone_shares_state() {
        let active1 = ActiveExecutions::new();
        let active2 = active1.clone();
        let run_id = Uuid::now_v7();

        // Spawn a task that waits before completing
        active1.spawn(run_id, async {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        });

        // Both should see the same state
        assert!(active2.contains(&run_id));
        assert_eq!(active2.count(), 1);

        // Clean up
        active1.shutdown();
    }
}
