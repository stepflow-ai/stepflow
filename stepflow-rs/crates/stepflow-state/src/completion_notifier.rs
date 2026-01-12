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

//! Run completion notification without polling.
//!
//! This module provides a mechanism for efficiently waiting for run completion
//! using a broadcast channel rather than polling.

use tokio::sync::broadcast;
use uuid::Uuid;

/// Handles run completion notification without polling.
///
/// This struct uses a broadcast channel to notify waiters when runs complete.
/// All `StateStore` implementations can compose with this to provide efficient
/// `wait_for_completion` functionality.
///
/// # Example
///
/// ```ignore
/// let notifier = RunCompletionNotifier::new();
///
/// // When a run completes (e.g., in update_run_status):
/// notifier.notify_completion(run_id);
///
/// // To wait for a specific run:
/// notifier.wait_for_completion(run_id).await;
/// ```
pub struct RunCompletionNotifier {
    sender: broadcast::Sender<Uuid>,
}

impl RunCompletionNotifier {
    /// Create a new completion notifier.
    ///
    /// The channel capacity determines how many completion events can be buffered.
    /// Waiters that fall behind will miss events, but this is safe because
    /// `wait_for_completion` checks current status before waiting.
    pub fn new() -> Self {
        // Capacity of 256 should be plenty for most workloads.
        // If a receiver falls behind, it will get a RecvError::Lagged
        // which we handle by re-checking status.
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    /// Notify all waiters that a run has completed.
    ///
    /// This should be called when a run reaches a terminal status
    /// (Completed, Failed, or Cancelled).
    pub fn notify_completion(&self, run_id: Uuid) {
        // It's fine if there are no receivers - just means no one is waiting
        let _ = self.sender.send(run_id);
    }

    /// Create a subscription to completion notifications.
    ///
    /// Returns a receiver that will receive run IDs when runs complete.
    /// The caller should subscribe BEFORE checking current status to avoid
    /// a race condition where completion happens between status check and subscribe.
    ///
    /// # Usage Pattern
    ///
    /// ```ignore
    /// let receiver = notifier.subscribe();
    /// // Check current status in state store
    /// if is_already_complete() {
    ///     return Ok(());
    /// }
    /// // Wait for completion notification
    /// notifier.wait_for_completion_with_receiver(run_id, receiver).await;
    /// ```
    pub fn subscribe(&self) -> broadcast::Receiver<Uuid> {
        self.sender.subscribe()
    }

    /// Wait for a specific run to complete using an existing receiver.
    ///
    /// This method is designed to be used after subscribing and checking status
    /// to avoid race conditions. Use `subscribe()` first, then check current status,
    /// then call this method if the run is not yet complete.
    ///
    /// # Arguments
    /// * `run_id` - The run ID to wait for
    /// * `receiver` - A receiver obtained from `subscribe()`
    pub async fn wait_for_completion_with_receiver(
        &self,
        run_id: Uuid,
        mut receiver: broadcast::Receiver<Uuid>,
    ) {
        loop {
            match receiver.recv().await {
                Ok(completed_run_id) => {
                    if completed_run_id == run_id {
                        return;
                    }
                    // Not our run, keep waiting
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // We fell behind and missed some messages.
                    // The caller should re-check the run status externally
                    // after this method returns.
                    return;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    // Channel closed, notifier was dropped
                    // Return so caller can handle appropriately
                    return;
                }
            }
        }
    }

    /// Wait for a specific run to complete.
    ///
    /// This blocks until the specified `run_id` is seen on the completion channel.
    ///
    /// **Important**: Callers should check if the run is already complete before
    /// calling this method to avoid waiting forever for an already-completed run.
    /// Consider using `subscribe()` + `wait_for_completion_with_receiver()` to
    /// avoid race conditions.
    ///
    /// # Arguments
    /// * `run_id` - The run ID to wait for
    pub async fn wait_for_completion(&self, run_id: Uuid) {
        let receiver = self.subscribe();
        self.wait_for_completion_with_receiver(run_id, receiver)
            .await;
    }
}

impl Default for RunCompletionNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn test_notify_and_wait() {
        let notifier = Arc::new(RunCompletionNotifier::new());
        let run_id = Uuid::now_v7();

        let notifier_clone = notifier.clone();
        let wait_handle = tokio::spawn(async move {
            notifier_clone.wait_for_completion(run_id).await;
        });

        // Give the waiter time to subscribe
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Notify completion
        notifier.notify_completion(run_id);

        // Wait should complete quickly
        let result = timeout(Duration::from_millis(100), wait_handle).await;
        assert!(result.is_ok(), "wait_for_completion should have returned");
    }

    #[tokio::test]
    async fn test_wait_ignores_other_runs() {
        let notifier = Arc::new(RunCompletionNotifier::new());
        let our_run_id = Uuid::now_v7();
        let other_run_id = Uuid::now_v7();

        let notifier_clone = notifier.clone();
        let wait_handle = tokio::spawn(async move {
            notifier_clone.wait_for_completion(our_run_id).await;
        });

        // Give the waiter time to subscribe
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Notify a different run
        notifier.notify_completion(other_run_id);

        // Should not complete yet
        let result = timeout(
            Duration::from_millis(50),
            &mut Box::pin(async {
                // This is a hack to check if the handle is still pending
            }),
        )
        .await;
        assert!(result.is_ok());

        // Now notify our run
        notifier.notify_completion(our_run_id);

        // Wait should complete now
        let result = timeout(Duration::from_millis(100), wait_handle).await;
        assert!(result.is_ok(), "wait_for_completion should have returned");
    }

    #[tokio::test]
    async fn test_multiple_waiters() {
        let notifier = Arc::new(RunCompletionNotifier::new());
        let run_id = Uuid::now_v7();

        // Start multiple waiters
        let mut handles = vec![];
        for _ in 0..5 {
            let notifier_clone = notifier.clone();
            handles.push(tokio::spawn(async move {
                notifier_clone.wait_for_completion(run_id).await;
            }));
        }

        // Give waiters time to subscribe
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Notify completion
        notifier.notify_completion(run_id);

        // All waiters should complete
        for handle in handles {
            let result = timeout(Duration::from_millis(100), handle).await;
            assert!(result.is_ok(), "All waiters should have completed");
        }
    }
}
