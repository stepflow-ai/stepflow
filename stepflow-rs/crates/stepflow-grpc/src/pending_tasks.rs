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

//! Two-phase task tracking for queue-based component execution.
//!
//! [`PendingTasks`] tracks task lifecycle through two phases:
//!
//! 1. **Queued** — task dispatched, waiting for `StartTask` from a worker.
//!    Subject to a per-task queue timeout set at [`register`] time.
//!
//! 2. **Executing** — worker called `StartTask`, now running the component.
//!    Subject to a heartbeat timeout (reset by each `TaskHeartbeat`).
//!    Optionally subject to an execution timeout cap.
//!
//! # Design
//!
//! Uses lock-free concurrent structures for all hot paths:
//! - [`DashMap`] for the task table (sharded, no global lock).
//! - `HeartbeatTracker` wraps a `DashSet`: tasks insert on heartbeat,
//!   scanner clears each cycle.
//! - `DelayQueue` from `tokio_util::time`: per-item timeouts tracked in the
//!   scanner task, yielding expired task IDs in deadline order.
//!
//! A single background task drives both timeout mechanisms via `tokio::select!`:
//! - **Heartbeat branch** — fires every [`HEARTBEAT_TIMEOUT`], collects
//!   executing tasks that did *not* appear in `HeartbeatTracker` during the
//!   interval and fails them.  Also handles execution-timeout caps.
//! - **Deadline branch** — fires whenever a task's queue timeout expires,
//!   fails the task if it is still in the Queued phase (lazy deletion via
//!   `remove_if`).
//!
//! The scanner task is stored in `PendingTasks` and aborted on [`shutdown`] or
//! when the last `Arc<PendingTasks>` is dropped.
//!
//! [`register`]: PendingTasks::register
//! [`shutdown`]: PendingTasks::shutdown

mod heartbeat_tracker;

use heartbeat_tracker::HeartbeatTracker;

use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use dashmap::DashMap;
use futures::StreamExt as _;
use stepflow_core::{ErrorCode, FlowError, FlowResult};
use tokio::sync::oneshot;
use tokio::time::MissedTickBehavior;

/// Heartbeat crash-detection timeout. Executing tasks that do not send a
/// heartbeat or CompleteTask within this window are presumed crashed.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(5);

// ============================================================================
// Task phase
// ============================================================================

/// Task phase in the lifecycle state machine.
enum TaskPhase {
    /// Task dispatched, waiting for worker to call StartTask.
    Queued { dispatched_at: tokio::time::Instant },
    /// Worker called StartTask, executing component.
    Executing {
        started_at: tokio::time::Instant,
        /// Optional cap on total execution time (measured from StartTask).
        execution_timeout: Option<Duration>,
    },
}

// Note: there is no `Done` variant. When a task completes or times out
// its entry is removed from the map entirely.

/// Internal entry for a tracked task.
struct TaskEntry {
    phase: TaskPhase,
    sender: Option<oneshot::Sender<FlowResult>>,
    /// Component path for metrics labeling.
    component: String,
    /// Optional cap on total execution time, stored at registration and applied
    /// when the worker calls StartTask.
    execution_timeout: Option<Duration>,
}

/// Reason a task timed out, for metrics.
#[derive(Debug, Clone, Copy)]
pub enum TimeoutKind {
    /// Timed out waiting for StartTask.
    Queue,
    /// Timed out waiting for heartbeat (worker presumed crashed).
    Heartbeat,
    /// Exceeded execution timeout cap.
    Execution,
}

// ============================================================================
// PendingTasks
// ============================================================================

/// Two-phase task tracking registry.
///
/// Shared between `StepflowQueuePlugin` (registers tasks) and
/// `OrchestratorServiceImpl` (handles StartTask, TaskHeartbeat,
/// CompleteTask RPCs).
///
/// Returns `Arc<Self>` from `new` because the background scanner holds a
/// `Weak` reference and exits when the last `Arc` is dropped (or on explicit
/// [`shutdown`]).
///
/// [`shutdown`]: PendingTasks::shutdown
pub struct PendingTasks {
    tasks: DashMap<String, TaskEntry>,
    heartbeat_tracker: HeartbeatTracker,
    /// Sender for queue-timeout deadlines. Per-task timeouts are pushed at
    /// [`register`] time; the `DelayQueue` in the scanner yields them in
    /// deadline order regardless of push order.
    ///
    /// [`register`]: PendingTasks::register
    deadline_tx: tokio::sync::mpsc::UnboundedSender<(String, Duration)>,
    /// Background scanner task (heartbeat + queue timeout combined).
    /// Aborted on [`shutdown`] or [`Drop`].
    ///
    /// [`shutdown`]: PendingTasks::shutdown
    scanner: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl std::fmt::Debug for PendingTasks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingTasks")
            .field("pending_count", &self.tasks.len())
            .finish()
    }
}

impl Drop for PendingTasks {
    fn drop(&mut self) {
        if let Some(handle) = self.scanner.get_mut().unwrap().take() {
            handle.abort();
        }
    }
}

impl PendingTasks {
    /// Create a new registry and start the background scanner task.
    pub fn new() -> Arc<Self> {
        let (deadline_tx, deadline_rx) = tokio::sync::mpsc::unbounded_channel();

        let registry = Arc::new(Self {
            tasks: DashMap::new(),
            heartbeat_tracker: HeartbeatTracker::new(),
            deadline_tx,
            scanner: Mutex::new(None),
        });

        let weak = Arc::downgrade(&registry);
        let handle = tokio::spawn(Self::scanner_task(weak, deadline_rx));
        *registry.scanner.lock().expect("scanner lock poisoned") = Some(handle);

        registry
    }

    /// Abort the background scanner task.
    ///
    /// No further timeouts will fire after this returns. Existing registered
    /// tasks remain in the map and can still be completed via [`complete`].
    ///
    /// [`complete`]: PendingTasks::complete
    pub fn shutdown(&self) {
        if let Some(handle) = self.scanner.lock().expect("scanner lock poisoned").take() {
            handle.abort();
        }
    }

    /// Register a new task in the Queued phase.
    ///
    /// - `queue_timeout` — how long the task may wait before a worker calls
    ///   StartTask.  Must be greater than zero.
    /// - `execution_timeout` — optional cap on execution time from StartTask
    ///   to CompleteTask.  `None` means no cap (heartbeat-only crash detection).
    ///
    /// Returns the receiver that will deliver the final result (success,
    /// failure, or timeout).
    pub fn register(
        &self,
        task_id: String,
        component: String,
        queue_timeout: Duration,
        execution_timeout: Option<Duration>,
    ) -> oneshot::Receiver<FlowResult> {
        debug_assert!(!queue_timeout.is_zero(), "queue_timeout must be > 0");

        let (tx, rx) = oneshot::channel();

        self.tasks.insert(
            task_id.clone(),
            TaskEntry {
                phase: TaskPhase::Queued {
                    dispatched_at: tokio::time::Instant::now(),
                },
                sender: Some(tx),
                component,
                execution_timeout,
            },
        );

        if self.deadline_tx.send((task_id, queue_timeout)).is_err() {
            log::debug!("deadline_tx send failed: scanner task has shut down");
        }

        rx
    }

    /// Transition a task from Queued to Executing (called by StartTask RPC).
    ///
    /// The execution timeout stored at [`register`] time is applied here.
    ///
    /// Returns:
    /// - `Some(false)` — task transitioned successfully, worker should execute.
    /// - `Some(true)` — task already started (idempotent).
    /// - `None` — task_id not found (already timed out or completed).
    ///
    /// [`register`]: PendingTasks::register
    pub fn start_task(&self, task_id: &str) -> Option<bool> {
        let mut entry = self.tasks.get_mut(task_id)?;

        match &entry.phase {
            TaskPhase::Queued { dispatched_at } => {
                stepflow_observability::metrics::record_task_queue_duration(
                    &entry.component,
                    dispatched_at.elapsed().as_secs_f64(),
                );

                let execution_timeout = entry.execution_timeout;
                entry.phase = TaskPhase::Executing {
                    started_at: tokio::time::Instant::now(),
                    execution_timeout,
                };
                // The queue deadline entry becomes stale; it will be skipped
                // lazily by the queue timeout scanner (remove_if in timeout_task).
                Some(false)
            }
            TaskPhase::Executing { .. } => {
                // Idempotent — already started, not an error.
                Some(false)
            }
        }
    }

    /// Record a heartbeat from the worker (called by TaskHeartbeat RPC).
    ///
    /// Inserts `task_id` into the heartbeat tracker so the next scan knows
    /// this task is still alive.
    ///
    /// Returns:
    /// - `Some(false)` — heartbeat recorded, `should_cancel = false`.
    /// - `Some(true)` — heartbeat recorded, `should_cancel = true` (future use).
    /// - `None` — task_id not found or not in Executing phase.
    pub fn heartbeat(&self, task_id: &str) -> Option<bool> {
        let entry = self.tasks.get(task_id)?;
        match &entry.phase {
            TaskPhase::Executing { .. } => {
                self.heartbeat_tracker.record(task_id);
                Some(false)
            }
            _ => None,
        }
    }

    /// Complete a task by sending its result to the waiting caller.
    ///
    /// Returns `true` if the task was found and the result delivered,
    /// `false` if the task_id was not found (already timed out or removed).
    pub fn complete(&self, task_id: &str, result: FlowResult) -> bool {
        let removed = self.tasks.remove(task_id);
        self.heartbeat_tracker.remove(task_id);

        let Some((_, mut entry)) = removed else {
            return false;
        };

        match &entry.phase {
            TaskPhase::Executing { started_at, .. } => {
                stepflow_observability::metrics::record_task_execution_duration(
                    &entry.component,
                    started_at.elapsed().as_secs_f64(),
                );
            }
            TaskPhase::Queued { dispatched_at } => {
                // Completed without StartTask (direct CompleteTask).
                stepflow_observability::metrics::record_task_queue_duration(
                    &entry.component,
                    dispatched_at.elapsed().as_secs_f64(),
                );
            }
        }

        match &result {
            FlowResult::Success(_) => {
                stepflow_observability::metrics::record_task_success(&entry.component);
            }
            FlowResult::Failed(_) => {
                stepflow_observability::metrics::record_task_failure(&entry.component);
            }
        }

        if let Some(sender) = entry.sender.take() {
            let _ = sender.send(result);
        }
        true
    }

    /// Remove a task registration without completing it.
    ///
    /// Used when the plugin side decides to cancel or clean up.
    pub fn remove(&self, task_id: &str) {
        self.tasks.remove(task_id);
        self.heartbeat_tracker.remove(task_id);
    }

    /// Number of tasks currently tracked (any phase).
    pub fn pending_count(&self) -> usize {
        self.tasks.len()
    }

    // --- Internal ---

    /// Fail a task due to timeout and emit the appropriate metric.
    ///
    /// For `Queue` timeouts, uses `remove_if` to only remove the entry when
    /// it is still in the Queued phase — preventing a race where StartTask
    /// arrives just as the deadline fires.
    ///
    /// For `Heartbeat` / `Execution` timeouts, removes unconditionally
    /// (the scanner only calls this for tasks it observed in Executing phase).
    fn timeout_task(&self, task_id: &str, kind: TimeoutKind) {
        let removed = match kind {
            TimeoutKind::Queue => {
                // Atomic: only remove if the task is still waiting for StartTask.
                self.tasks
                    .remove_if(task_id, |_, e| matches!(e.phase, TaskPhase::Queued { .. }))
            }
            TimeoutKind::Heartbeat | TimeoutKind::Execution => self.tasks.remove(task_id),
        };
        self.heartbeat_tracker.remove(task_id);

        let Some((_, mut entry)) = removed else {
            return; // Task already completed, started, or removed.
        };

        let message = match kind {
            TimeoutKind::Queue => {
                stepflow_observability::metrics::record_task_queue_timeout(&entry.component);
                format!("task '{task_id}' timed out waiting for worker to start")
            }
            TimeoutKind::Heartbeat => {
                stepflow_observability::metrics::record_task_execution_timeout(&entry.component);
                format!(
                    "task '{task_id}' timed out: no heartbeat received \
                     (worker presumed crashed)"
                )
            }
            TimeoutKind::Execution => {
                stepflow_observability::metrics::record_task_execution_timeout(&entry.component);
                format!("task '{task_id}' exceeded execution timeout")
            }
        };

        log::warn!("{message}");

        // Use transport error code so the retry system classifies these as
        // transport errors and triggers subprocess restart via prepare_for_retry().
        if let Some(sender) = entry.sender.take() {
            let _ = sender.send(FlowResult::Failed(FlowError::new(
                ErrorCode::TRANSPORT_ERROR,
                message,
            )));
        }
    }

    /// Heartbeat scanner body — called once per `HEARTBEAT_TIMEOUT` cycle.
    ///
    /// Collects executing tasks that did not appear in the heartbeat tracker
    /// since the last scan (missed heartbeat) or that exceeded their execution
    /// timeout cap, then times them out.  Resets the tracker before applying
    /// timeouts so new heartbeats during the timeout phase count toward the
    /// next cycle.
    fn scan_heartbeats(&self) {
        let mut timeout_heartbeat: Vec<String> = Vec::new();
        let mut timeout_execution: Vec<String> = Vec::new();

        for entry in self.tasks.iter() {
            if let TaskPhase::Executing {
                started_at,
                execution_timeout,
            } = &entry.phase
            {
                if execution_timeout.is_some_and(|et| started_at.elapsed() >= et) {
                    timeout_execution.push(entry.key().clone());
                } else if started_at.elapsed() >= HEARTBEAT_TIMEOUT
                    && !self.heartbeat_tracker.was_seen(entry.key())
                {
                    // Only timeout after grace period — newly started tasks
                    // haven't had time to send their first heartbeat yet.
                    timeout_heartbeat.push(entry.key().clone());
                }
            }
        }

        // Reset the tracker so heartbeats that arrive while we apply
        // timeouts count toward the next cycle.
        self.heartbeat_tracker.reset();

        for task_id in timeout_execution {
            self.timeout_task(&task_id, TimeoutKind::Execution);
        }
        for task_id in timeout_heartbeat {
            self.timeout_task(&task_id, TimeoutKind::Heartbeat);
        }
    }

    /// Combined background scanner — drives both the heartbeat and queue
    /// timeout mechanisms via a single `tokio::select!` loop.
    ///
    /// The heartbeat branch fires on a fixed `HEARTBEAT_TIMEOUT` interval.
    /// The deadline branch uses a `tokio_util::time::DelayQueue` to yield
    /// expired task IDs in deadline order.
    ///
    /// The scanner exits when:
    /// - The task is aborted (via [`shutdown`] or [`Drop`]).
    /// - The `Weak` reference to the registry can no longer be upgraded
    ///   (last `Arc` was dropped).
    /// - The `deadline_rx` channel is closed (all senders dropped).
    ///
    /// [`shutdown`]: PendingTasks::shutdown
    async fn scanner_task(
        registry: Weak<Self>,
        mut deadline_rx: tokio::sync::mpsc::UnboundedReceiver<(String, Duration)>,
    ) {
        let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_TIMEOUT);
        // Skip missed ticks — if a scan takes unexpectedly long we don't
        // want a burst of heartbeat scans when it catches up.
        heartbeat_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Consume the initial immediate tick so the first scan fires after
        // one full HEARTBEAT_TIMEOUT, matching the previous sleep-based behaviour.
        heartbeat_interval.tick().await;

        let mut delay_queue: tokio_util::time::DelayQueue<String> =
            tokio_util::time::DelayQueue::new();

        loop {
            tokio::select! {
                _ = heartbeat_interval.tick() => {
                    let Some(r) = registry.upgrade() else { return };
                    r.scan_heartbeats();
                }
                Some(expired) = delay_queue.next() => {
                    let task_id = expired.into_inner();
                    let Some(r) = registry.upgrade() else { return };
                    r.timeout_task(&task_id, TimeoutKind::Queue);
                }
                msg = deadline_rx.recv() => {
                    match msg {
                        Some((task_id, timeout)) => {
                            delay_queue.insert(task_id, timeout);
                        }
                        None => return, // all senders dropped (PendingTasks dropped)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stepflow_core::workflow::ValueRef;

    const QUEUE_TIMEOUT: Duration = Duration::from_millis(50);

    #[tokio::test]
    async fn test_register_and_complete() {
        let registry = PendingTasks::new();
        let rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );
        assert_eq!(registry.pending_count(), 1);

        let value = ValueRef::new(serde_json::json!({"result": "ok"}));
        assert!(registry.complete("task-1", FlowResult::Success(value.clone())));

        assert_eq!(registry.pending_count(), 0);
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    #[tokio::test]
    async fn test_complete_unknown_task() {
        let registry = PendingTasks::new();
        let value = ValueRef::new(serde_json::json!(null));
        assert!(!registry.complete("nonexistent", FlowResult::Success(value)));
    }

    #[tokio::test]
    async fn test_remove_task() {
        let registry = PendingTasks::new();
        let _rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );
        assert_eq!(registry.pending_count(), 1);
        registry.remove("task-1");
        assert_eq!(registry.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_receiver_dropped_before_complete() {
        let registry = PendingTasks::new();
        let rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );
        drop(rx);
        // complete() should succeed even though nobody is listening
        let value = ValueRef::new(serde_json::json!(null));
        assert!(registry.complete("task-1", FlowResult::Success(value)));
    }

    #[tokio::test]
    async fn test_full_lifecycle() {
        let registry = PendingTasks::new();
        let rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );

        assert!(registry.start_task("task-1").is_some());
        assert!(registry.heartbeat("task-1").is_some());

        let value = ValueRef::new(serde_json::json!({"done": true}));
        assert!(registry.complete("task-1", FlowResult::Success(value.clone())));
        assert_eq!(registry.pending_count(), 0);
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    #[tokio::test]
    async fn test_start_task_unknown_returns_none() {
        let registry = PendingTasks::new();
        assert!(registry.start_task("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_heartbeat_unknown_returns_none() {
        let registry = PendingTasks::new();
        assert!(registry.heartbeat("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_queue_timeout() {
        tokio::time::pause();

        let registry = PendingTasks::new();
        let rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            Duration::from_millis(50),
            None,
        );

        tokio::time::advance(Duration::from_millis(60)).await;
        tokio::task::yield_now().await;

        let FlowResult::Failed(err) = rx.await.unwrap() else {
            panic!("expected failure");
        };
        assert!(err.message.contains("timed out waiting for worker"));
        assert_eq!(registry.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_start_task_after_queue_timeout_returns_not_found() {
        tokio::time::pause();

        let registry = PendingTasks::new();
        let _rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            Duration::from_millis(50),
            None,
        );

        tokio::time::sleep(Duration::from_millis(60)).await;

        assert!(registry.start_task("task-1").is_none());
    }

    #[tokio::test]
    async fn test_heartbeat_timeout() {
        tokio::time::pause();

        let registry = PendingTasks::new();
        let rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );
        registry.start_task("task-1").unwrap();

        tokio::time::advance(HEARTBEAT_TIMEOUT + Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        let FlowResult::Failed(err) = rx.await.unwrap() else {
            panic!("expected failure");
        };
        assert!(err.message.contains("no heartbeat received"));
        assert_eq!(registry.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_heartbeat_resets_timeout() {
        tokio::time::pause();

        let registry = PendingTasks::new();
        let rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );
        registry.start_task("task-1").unwrap();

        // Send heartbeats every 2s for 12s — spans multiple 5s scan cycles.
        for _ in 0..6 {
            tokio::time::advance(Duration::from_secs(2)).await;
            tokio::task::yield_now().await;
            assert!(registry.heartbeat("task-1").is_some());
        }

        let value = ValueRef::new(serde_json::json!({"ok": true}));
        assert!(registry.complete("task-1", FlowResult::Success(value.clone())));
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    #[tokio::test]
    async fn test_execution_timeout() {
        tokio::time::pause();

        let registry = PendingTasks::new();
        let rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            Some(Duration::from_millis(200)),
        );
        registry.start_task("task-1").unwrap();

        // Heartbeat to keep the task alive past the heartbeat window,
        // but let the execution cap (200ms) expire.
        for _ in 0..10 {
            tokio::time::advance(Duration::from_millis(50)).await;
            tokio::task::yield_now().await;
            let _ = registry.heartbeat("task-1");
        }

        // Trigger the heartbeat scanner — it sees started_at.elapsed() >= 200ms.
        tokio::time::advance(HEARTBEAT_TIMEOUT).await;
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }

        let FlowResult::Failed(err) = rx.await.unwrap() else {
            panic!("expected failure");
        };
        assert!(
            err.message.contains("exceeded execution timeout"),
            "unexpected: {}",
            err.message,
        );
        assert_eq!(registry.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_many_tasks_all_cleaned_up_after_queue_timeout() {
        tokio::time::pause();

        let registry = PendingTasks::new();
        let _receivers: Vec<_> = (0..20)
            .map(|i| {
                registry.register(
                    format!("task-{i}"),
                    "test_component".to_string(),
                    Duration::from_millis(50),
                    None,
                )
            })
            .collect();

        assert_eq!(registry.pending_count(), 20);
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(registry.pending_count(), 0);
    }

    /// Verify that a queue timeout that fires just as StartTask is called
    /// does not incorrectly fail a task that is already executing.
    #[tokio::test]
    async fn test_start_task_beats_queue_timeout_race() {
        tokio::time::pause();

        let registry = PendingTasks::new();
        let rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            Duration::from_millis(50),
            None,
        );

        // StartTask before the queue timeout fires.
        registry.start_task("task-1").unwrap();

        // Let the queue deadline pass — timeout_task uses remove_if(Queued),
        // so the now-Executing task must NOT be failed.
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(registry.pending_count(), 1);

        let value = ValueRef::new(serde_json::json!({"ok": true}));
        assert!(registry.complete("task-1", FlowResult::Success(value.clone())));
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    /// Verify that a newly started task is not timed out by the heartbeat
    /// scanner before it has had a chance to send its first heartbeat.
    ///
    /// Regression test for: a task that transitions to Executing just before
    /// a heartbeat scan fires would be immediately timed out because the
    /// heartbeat tracker had no entry for it yet.
    #[tokio::test]
    async fn test_heartbeat_grace_period_for_newly_started_task() {
        tokio::time::pause();

        let registry = PendingTasks::new();
        let rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );

        // Advance past one heartbeat scan cycle so the scanner is primed.
        tokio::time::advance(HEARTBEAT_TIMEOUT + Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        // Start the task right before the next scan fires.
        registry.start_task("task-1").unwrap();

        // Advance a short time (less than HEARTBEAT_TIMEOUT) to trigger
        // the next scan cycle. The task has just started — no heartbeat
        // yet, but the grace period should protect it.
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        // Task must still be alive.
        assert_eq!(registry.pending_count(), 1);

        // Now send a heartbeat and complete normally.
        assert!(registry.heartbeat("task-1").is_some());
        let value = ValueRef::new(serde_json::json!({"ok": true}));
        assert!(registry.complete("task-1", FlowResult::Success(value.clone())));
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    #[tokio::test]
    async fn test_shutdown_stops_scanner() {
        tokio::time::pause();

        let registry = PendingTasks::new();
        let _rx = registry.register(
            "task-1".to_string(),
            "test_component".to_string(),
            Duration::from_millis(50),
            None,
        );

        // Shut down before the queue timeout would fire.
        registry.shutdown();

        // Advance past the timeout — scanner is gone, task stays registered.
        tokio::time::advance(Duration::from_millis(60)).await;
        tokio::task::yield_now().await;

        assert_eq!(
            registry.pending_count(),
            1,
            "scanner was shut down; task should remain"
        );
    }
}
