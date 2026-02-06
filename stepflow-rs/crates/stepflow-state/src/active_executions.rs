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
//! // Track an execution spawned elsewhere
//! let handle = tokio::spawn(async move { /* ... */ });
//! active.track(run_id, handle);
//!
//! // Check how many are running
//! println!("Active executions: {}", active.count());
//!
//! // Graceful shutdown
//! active.shutdown();
//! ```

use std::sync::Arc;

use dashmap::DashMap;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Tracks active workflow executions.
///
/// Each execution is tracked by its root run ID. When an execution completes,
/// it should call `remove()` to clean up its entry.
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

    /// Track an execution by its root run ID.
    ///
    /// The caller is responsible for calling `remove()` when the execution completes.
    ///
    /// # Arguments
    /// * `root_run_id` - The root run ID to track
    /// * `handle` - The task handle from `tokio::spawn`
    pub fn track(&self, root_run_id: Uuid, handle: JoinHandle<()>) {
        self.executions.insert(root_run_id, handle);
    }

    /// Remove an execution from tracking.
    ///
    /// Call this when an execution completes (success or failure).
    pub fn remove(&self, root_run_id: &Uuid) {
        self.executions.remove(root_run_id);
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

    /// Wait for all active executions to complete.
    ///
    /// This does not cancel any executions - it just waits for them to finish.
    /// Useful for graceful shutdown where you want to let in-progress work complete.
    pub async fn wait_for_all(&self) {
        // Collect handles to avoid holding the lock while awaiting
        let handles: Vec<_> = self
            .executions
            .iter()
            .map(|entry| *entry.key())
            .collect();

        for root_run_id in handles {
            // Wait for each to complete by checking if it's still in the map
            while self.executions.contains_key(&root_run_id) {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
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
    async fn test_track_and_remove() {
        let active = ActiveExecutions::new();
        let run_id = Uuid::now_v7();

        let handle = tokio::spawn(async {});
        active.track(run_id, handle);

        assert!(active.contains(&run_id));
        assert_eq!(active.count(), 1);

        active.remove(&run_id);
        assert!(!active.contains(&run_id));
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn test_shutdown() {
        let active = ActiveExecutions::new();

        // Track a long-running task
        let handle = tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        });
        let run_id = Uuid::now_v7();
        active.track(run_id, handle);

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
        let handle = tokio::spawn(async {});
        active1.track(run_id, handle);

        // Both should see the same state
        assert!(active2.contains(&run_id));
        assert_eq!(active2.count(), 1);
    }
}
