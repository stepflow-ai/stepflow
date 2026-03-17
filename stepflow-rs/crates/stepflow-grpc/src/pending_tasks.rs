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

//! gRPC-specific task lifecycle tracking for queue-based component execution.
//!
//! [`PendingTasks`] augments the shared [`TaskRegistry`] with gRPC-specific
//! concerns: phase transitions, heartbeat monitoring, and timeout detection.
//!
//! **Separation of concerns:**
//! - [`TaskRegistry`] (in `stepflow-plugin`) owns the oneshot channels for
//!   result delivery. The executor registers task_ids there and awaits results.
//! - `PendingTasks` (this module) tracks *how* each task is progressing through
//!   the gRPC lifecycle: phase (Queued → Executing), heartbeat liveness, and
//!   timeout deadlines. It holds its own `DashMap` keyed by task_id with only
//!   phase/timeout/heartbeat state — no oneshot channels.
//! - When a timeout fires, `PendingTasks` delivers a failure result through
//!   `TaskRegistry.complete()`. When a worker calls CompleteTask,
//!   `PendingTasks.complete()` records metrics, removes its tracking entry,
//!   and delegates result delivery to `TaskRegistry.complete()`.
//!
//! # Task lifecycle
//!
//! [`PendingTasks`] tracks task lifecycle through two phases:
//!
//! 1. **Queued** — task dispatched, waiting for first `TaskHeartbeat` from a worker.
//!    Subject to a per-task queue timeout set at [`track`] time.
//!
//! 2. **Executing** — worker sent first `TaskHeartbeat`, now running the component.
//!    Subject to a heartbeat timeout (reset by each `TaskHeartbeat`).
//!    Optionally subject to an execution timeout cap.
//!
//! # Implementation
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
//! [`TaskRegistry`]: stepflow_plugin::TaskRegistry
//! [`track`]: PendingTasks::track
//! [`shutdown`]: PendingTasks::shutdown

mod heartbeat_tracker;

use heartbeat_tracker::HeartbeatTracker;

use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use dashmap::DashMap;
use futures::StreamExt as _;
use stepflow_core::{FlowError, FlowResult};
use stepflow_plugin::TaskRegistry;
use tokio::time::MissedTickBehavior;

/// Heartbeat crash-detection timeout. Executing tasks that do not send a
/// heartbeat or CompleteTask within this window are presumed crashed.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(5);

// ============================================================================
// Task phase
// ============================================================================

/// Task phase in the lifecycle state machine.
enum TaskPhase {
    /// Task dispatched, waiting for worker to send first heartbeat.
    Queued { dispatched_at: tokio::time::Instant },
    /// Worker sent first heartbeat, executing component.
    Executing {
        started_at: tokio::time::Instant,
        /// Optional cap on total execution time (measured from first heartbeat).
        execution_timeout: Option<Duration>,
    },
}

// Note: there is no `Done` variant. When a task completes or times out
// its entry is removed from the map entirely.

/// Internal entry for a tracked task.
struct TaskEntry {
    phase: TaskPhase,
    /// Component path for metrics labeling.
    component: String,
    /// Optional cap on total execution time, stored at registration and applied
    /// when the worker sends its first heartbeat.
    execution_timeout: Option<Duration>,
    /// Worker that owns this task (set on first heartbeat).
    worker_id: Option<String>,
}

/// Result of a heartbeat call, indicating the task's status from the
/// orchestrator's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatResult {
    /// Task is in progress — the calling worker owns it.
    InProgress,
    /// Task is already being executed by a different worker.
    AlreadyClaimed,
    /// Task ID not recognized — never existed, already completed and
    /// cleaned up, or timed out.
    NotFound,
}

/// Reason a task timed out, for metrics.
#[derive(Debug, Clone, Copy)]
pub enum TimeoutKind {
    /// Timed out waiting for first heartbeat.
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
/// `OrchestratorServiceImpl` (handles TaskHeartbeat, CompleteTask RPCs).
///
/// Returns `Arc<Self>` from `new` because the background scanner holds a
/// `Weak` reference and exits when the last `Arc` is dropped (or on explicit
/// [`shutdown`]).
///
/// [`shutdown`]: PendingTasks::shutdown
pub struct PendingTasks {
    tasks: DashMap<String, TaskEntry>,
    heartbeat_tracker: HeartbeatTracker,
    /// Shared task registry for result delivery.
    task_registry: Arc<TaskRegistry>,
    /// Sender for queue-timeout deadlines. Per-task timeouts are pushed at
    /// [`track`] time; the `DelayQueue` in the scanner yields them in
    /// deadline order regardless of push order.
    ///
    /// [`track`]: PendingTasks::track
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
    ///
    /// The `task_registry` is the shared in-memory registry used for result
    /// delivery. Timeouts detected by this tracker will deliver failures
    /// through the registry.
    pub fn new(task_registry: Arc<TaskRegistry>) -> Arc<Self> {
        let (deadline_tx, deadline_rx) = tokio::sync::mpsc::unbounded_channel();

        let registry = Arc::new(Self {
            tasks: DashMap::new(),
            heartbeat_tracker: HeartbeatTracker::new(),
            task_registry,
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
    /// No further timeouts will fire after this returns. Existing tracked
    /// tasks remain in the map and can still be completed via [`complete`].
    ///
    /// [`complete`]: PendingTasks::complete
    pub fn shutdown(&self) {
        if let Some(handle) = self.scanner.lock().expect("scanner lock poisoned").take() {
            handle.abort();
        }
    }

    /// Start tracking a task's gRPC lifecycle (phase transitions, timeouts,
    /// heartbeats).
    ///
    /// The task must already be registered in the shared [`TaskRegistry`]
    /// by the executor. This method only sets up gRPC-specific timeout and
    /// heartbeat tracking.
    ///
    /// - `queue_timeout` — how long the task may wait before a worker sends
    ///   its first heartbeat.  Must be greater than zero.
    /// - `execution_timeout` — optional cap on execution time from first
    ///   heartbeat to CompleteTask.  `None` means no cap (heartbeat-only
    ///   crash detection).
    pub fn track(
        &self,
        task_id: String,
        component: String,
        queue_timeout: Duration,
        execution_timeout: Option<Duration>,
    ) {
        debug_assert!(!queue_timeout.is_zero(), "queue_timeout must be > 0");

        self.tasks.insert(
            task_id.clone(),
            TaskEntry {
                phase: TaskPhase::Queued {
                    dispatched_at: tokio::time::Instant::now(),
                },
                component,
                execution_timeout,
                worker_id: None,
            },
        );

        if self.deadline_tx.send((task_id, queue_timeout)).is_err() {
            log::debug!("deadline_tx send failed: scanner task has shut down");
        }
    }

    /// Remove tracking for a task without completing it.
    ///
    /// Used when task dispatch fails and tracking should be cleaned up.
    /// Does NOT remove the task from the shared TaskRegistry (the executor
    /// handles that).
    pub fn untrack(&self, task_id: &str) {
        self.tasks.remove(task_id);
        self.heartbeat_tracker.remove(task_id);
    }

    /// Record a heartbeat from a worker (called by TaskHeartbeat RPC).
    ///
    /// This is the unified start + heartbeat method. On first call (task in
    /// Queued phase), transitions the task to Executing and records the
    /// `worker_id`. On subsequent calls, verifies the `worker_id` matches
    /// and resets the heartbeat timer.
    ///
    /// Returns a [`HeartbeatResult`] indicating what the worker should do.
    pub fn heartbeat(&self, task_id: &str, worker_id: &str) -> HeartbeatResult {
        let Some(mut entry) = self.tasks.get_mut(task_id) else {
            return HeartbeatResult::NotFound;
        };

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
                entry.worker_id = Some(worker_id.to_string());
                // The queue deadline entry becomes stale; it will be skipped
                // lazily by the queue timeout scanner (remove_if in timeout_task).
                self.heartbeat_tracker.record(task_id);
                HeartbeatResult::InProgress
            }
            TaskPhase::Executing { .. } => {
                // Check worker_id matches
                if entry.worker_id.as_deref() == Some(worker_id) {
                    self.heartbeat_tracker.record(task_id);
                    HeartbeatResult::InProgress
                } else {
                    HeartbeatResult::AlreadyClaimed
                }
            }
        }
    }

    /// Complete a task: record metrics, remove tracking, and deliver the
    /// result via the shared [`TaskRegistry`].
    ///
    /// Returns `true` if the result was delivered to the registry (the
    /// executor received the result), `false` if the task_id was not found
    /// in the registry (already completed, timed out, or removed by the
    /// executor).
    pub fn complete(&self, task_id: &str, result: FlowResult) -> bool {
        let removed = self.tasks.remove(task_id);
        self.heartbeat_tracker.remove(task_id);

        // Record metrics if we were tracking this task.
        if let Some((_, entry)) = removed {
            match &entry.phase {
                TaskPhase::Executing { started_at, .. } => {
                    stepflow_observability::metrics::record_task_execution_duration(
                        &entry.component,
                        started_at.elapsed().as_secs_f64(),
                    );
                }
                TaskPhase::Queued { dispatched_at } => {
                    // Completed without heartbeat (direct CompleteTask).
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
        }

        // Deliver via the shared registry. Returns false if the executor
        // no longer has a listener for this task_id (e.g., already timed
        // out or completed via another path).
        self.task_registry.complete(task_id, result)
    }

    /// Number of tasks currently tracked (any phase).
    pub fn pending_count(&self) -> usize {
        self.tasks.len()
    }

    // --- Internal ---

    /// Fail a task due to timeout and emit the appropriate metric.
    ///
    /// For `Queue` timeouts, uses `remove_if` to only remove the entry when
    /// it is still in the Queued phase — preventing a race where the first
    /// heartbeat arrives just as the deadline fires.
    ///
    /// For `Heartbeat` / `Execution` timeouts, removes unconditionally
    /// (the scanner only calls this for tasks it observed in Executing phase).
    fn timeout_task(&self, task_id: &str, kind: TimeoutKind) {
        let removed = match kind {
            TimeoutKind::Queue => {
                // Atomic: only remove if the task is still waiting for first heartbeat.
                self.tasks
                    .remove_if(task_id, |_, e| matches!(e.phase, TaskPhase::Queued { .. }))
            }
            TimeoutKind::Heartbeat | TimeoutKind::Execution => self.tasks.remove(task_id),
        };
        self.heartbeat_tracker.remove(task_id);

        let Some((_, entry)) = removed else {
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

        // Deliver timeout failure via the shared TaskRegistry. Uses Timeout
        // error code so the retry system classifies these as transport errors
        // and triggers subprocess restart via prepare_for_retry().
        self.task_registry.complete(
            task_id,
            FlowResult::Failed(FlowError::new(
                stepflow_core::TaskErrorCode::Timeout,
                message,
            )),
        );
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
    use stepflow_core::TaskErrorCode;
    use stepflow_core::workflow::ValueRef;

    const QUEUE_TIMEOUT: Duration = Duration::from_millis(50);

    /// Helper: create a TaskRegistry + PendingTasks pair, track a task,
    /// and return the (pending_tasks, task_registry_rx) tuple.
    fn setup() -> (Arc<PendingTasks>, Arc<TaskRegistry>) {
        let task_registry = Arc::new(TaskRegistry::new());
        let pending = PendingTasks::new(task_registry.clone());
        (pending, task_registry)
    }

    #[tokio::test]
    async fn test_track_and_complete() {
        let (pending, task_registry) = setup();
        let rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );
        assert_eq!(pending.pending_count(), 1);

        let value = ValueRef::new(serde_json::json!({"result": "ok"}));
        assert!(pending.complete("task-1", FlowResult::Success(value.clone())));

        assert_eq!(pending.pending_count(), 0);
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    #[tokio::test]
    async fn test_complete_unknown_task() {
        let (pending, _task_registry) = setup();
        let value = ValueRef::new(serde_json::json!(null));
        assert!(!pending.complete("nonexistent", FlowResult::Success(value)));
    }

    #[tokio::test]
    async fn test_untrack_task() {
        let (pending, task_registry) = setup();
        let _rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );
        assert_eq!(pending.pending_count(), 1);
        pending.untrack("task-1");
        assert_eq!(pending.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_full_lifecycle() {
        let (pending, task_registry) = setup();
        let rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );

        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );
        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );

        let value = ValueRef::new(serde_json::json!({"done": true}));
        assert!(pending.complete("task-1", FlowResult::Success(value.clone())));
        assert_eq!(pending.pending_count(), 0);
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    #[tokio::test]
    async fn test_heartbeat_unknown_returns_not_found() {
        let (pending, _task_registry) = setup();
        assert_eq!(
            pending.heartbeat("nonexistent", "worker-1"),
            HeartbeatResult::NotFound
        );
    }

    #[tokio::test]
    async fn test_heartbeat_already_claimed() {
        let (pending, task_registry) = setup();
        let _rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );

        // First worker claims the task
        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );

        // Different worker tries to heartbeat — should be rejected
        assert_eq!(
            pending.heartbeat("task-1", "worker-2"),
            HeartbeatResult::AlreadyClaimed
        );

        // Original worker can still heartbeat
        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );
    }

    #[tokio::test]
    async fn test_queue_timeout() {
        tokio::time::pause();

        let (pending, task_registry) = setup();
        let rx = task_registry.register("task-1".to_string());
        pending.track(
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
        assert_eq!(err.code, TaskErrorCode::Timeout);
        assert_eq!(pending.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_heartbeat_after_queue_timeout_returns_not_found() {
        tokio::time::pause();

        let (pending, task_registry) = setup();
        let _rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            Duration::from_millis(50),
            None,
        );

        tokio::time::sleep(Duration::from_millis(60)).await;

        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::NotFound
        );
    }

    #[tokio::test]
    async fn test_heartbeat_timeout() {
        tokio::time::pause();

        let (pending, task_registry) = setup();
        let rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );
        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );

        tokio::time::advance(HEARTBEAT_TIMEOUT + Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        let FlowResult::Failed(err) = rx.await.unwrap() else {
            panic!("expected failure");
        };
        assert!(err.message.contains("no heartbeat received"));
        assert_eq!(err.code, TaskErrorCode::Timeout);
        assert_eq!(pending.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_heartbeat_resets_timeout() {
        tokio::time::pause();

        let (pending, task_registry) = setup();
        let rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );
        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );

        // Send heartbeats every 2s for 12s — spans multiple 5s scan cycles.
        for _ in 0..6 {
            tokio::time::advance(Duration::from_secs(2)).await;
            tokio::task::yield_now().await;
            assert_eq!(
                pending.heartbeat("task-1", "worker-1"),
                HeartbeatResult::InProgress
            );
        }

        let value = ValueRef::new(serde_json::json!({"ok": true}));
        assert!(pending.complete("task-1", FlowResult::Success(value.clone())));
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    #[tokio::test]
    async fn test_execution_timeout() {
        tokio::time::pause();

        let (pending, task_registry) = setup();
        let rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            Some(Duration::from_millis(200)),
        );
        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );

        for _ in 0..10 {
            tokio::time::advance(Duration::from_millis(50)).await;
            tokio::task::yield_now().await;
            let _ = pending.heartbeat("task-1", "worker-1");
        }

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
            err.message
        );
        assert_eq!(err.code, TaskErrorCode::Timeout);
        assert_eq!(pending.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_many_tasks_all_cleaned_up_after_queue_timeout() {
        tokio::time::pause();

        let (pending, task_registry) = setup();
        for i in 0..20 {
            let _rx = task_registry.register(format!("task-{i}"));
            pending.track(
                format!("task-{i}"),
                "test_component".to_string(),
                Duration::from_millis(50),
                None,
            );
        }

        assert_eq!(pending.pending_count(), 20);
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(pending.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_heartbeat_beats_queue_timeout_race() {
        tokio::time::pause();

        let (pending, task_registry) = setup();
        let rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            Duration::from_millis(50),
            None,
        );

        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );

        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(pending.pending_count(), 1);

        let value = ValueRef::new(serde_json::json!({"ok": true}));
        assert!(pending.complete("task-1", FlowResult::Success(value.clone())));
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    #[tokio::test]
    async fn test_heartbeat_grace_period_for_newly_started_task() {
        tokio::time::pause();

        let (pending, task_registry) = setup();
        let rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            QUEUE_TIMEOUT,
            None,
        );

        tokio::time::advance(HEARTBEAT_TIMEOUT + Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );

        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;

        assert_eq!(pending.pending_count(), 1);

        assert_eq!(
            pending.heartbeat("task-1", "worker-1"),
            HeartbeatResult::InProgress
        );
        let value = ValueRef::new(serde_json::json!({"ok": true}));
        assert!(pending.complete("task-1", FlowResult::Success(value.clone())));
        assert_eq!(rx.await.unwrap(), FlowResult::Success(value));
    }

    #[tokio::test]
    async fn test_shutdown_stops_scanner() {
        tokio::time::pause();

        let (pending, task_registry) = setup();
        let _rx = task_registry.register("task-1".to_string());
        pending.track(
            "task-1".to_string(),
            "test_component".to_string(),
            Duration::from_millis(50),
            None,
        );

        pending.shutdown();

        tokio::time::advance(Duration::from_millis(60)).await;
        tokio::task::yield_now().await;

        assert_eq!(
            pending.pending_count(),
            1,
            "scanner was shut down; task should remain"
        );
    }
}
