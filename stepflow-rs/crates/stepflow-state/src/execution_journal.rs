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

//! ExecutionJournal trait for write-ahead logging of execution events.
//!
//! This trait handles recording execution events that can be replayed to recover
//! run state after a crash. It is designed to support both local journalling
//! (SQLite) and distributed journalling (NATS JetStream).
//!
//! The journal records fine-grained execution events (task starts, completions,
//! dependency unblocking) that are used to reconstruct RunState during recovery.
//!
//! ## Journal Organization
//!
//! Journals are keyed by `root_run_id`, meaning all events for a workflow execution
//! tree (parent flow + all subflows) are stored in a single journal. This simplifies:
//!
//! - **Recovery**: Load one journal to reconstruct the entire execution tree
//! - **Garbage collection**: Delete one journal when the root run completes
//! - **Ordering**: Single sequence number space provides total ordering across all runs
//!
//! Each entry contains a `run_id` field to identify which specific run (parent or
//! subflow) the event belongs to, enabling filtering during replay.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use std::pin::Pin;

use futures::Stream;
use futures::future::{BoxFuture, FutureExt as _};
use serde::{Deserialize, Serialize};
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::ValueRef;
use stepflow_core::{BlobId, FlowResult};
use uuid::Uuid;

use crate::StateError;

/// A stream of journal entries, used for iterating over journal contents
/// during recovery and for SSE streaming.
///
/// Implementations yield [`JournalEntry`] values in sequence order. Errors are
/// propagated as stream items, allowing the consumer to handle them inline.
pub type JournalEventStream<'a> =
    Pin<Box<dyn Stream<Item = error_stack::Result<JournalEntry, StateError>> + Send + 'a>>;

/// Sequence number for journal entries.
///
/// Sequence numbers are monotonically increasing within each root run's journal
/// and are used to track replay position during recovery.
///
/// Note: Sequence numbers don't need to start at 0. Implementations may use
/// backend-specific offsets (e.g., Kafka offsets). Recovery should read from
/// the first available sequence for a given root_run_id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SequenceNumber(pub u64);

impl SequenceNumber {
    /// Create a new sequence number.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Get the next sequence number.
    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    /// Get the underlying value.
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for SequenceNumber {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<SequenceNumber> for u64 {
    fn from(seq: SequenceNumber) -> Self {
        seq.0
    }
}

/// Information about a root run's journal state.
///
/// Since journals are keyed by `root_run_id`, this represents the journal
/// for an entire execution tree (parent flow + all subflows).
#[derive(Debug, Clone)]
pub struct RootJournalInfo {
    /// The root run ID (journal key).
    pub root_run_id: Uuid,
    /// The latest sequence number in the journal.
    pub latest_sequence: SequenceNumber,
    /// Number of entries in the journal.
    pub entry_count: u64,
}

/// A journal entry containing a timestamped execution event.
///
/// Entries are stored in a journal keyed by `root_run_id`. The `run_id` field
/// identifies which specific run (parent or subflow) the event belongs to,
/// enabling filtering during replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// The specific run this entry belongs to (may be parent or subflow).
    pub run_id: Uuid,
    /// The root run ID (journal key, same for all entries in the tree).
    pub root_run_id: Uuid,
    /// Journal sequence number assigned by the storage backend.
    pub sequence: SequenceNumber,
    /// When this event occurred.
    pub timestamp: DateTime<Utc>,
    /// The execution event.
    pub event: JournalEvent,
}

impl JournalEntry {
    /// Create a new journal entry with the current timestamp and a placeholder sequence.
    ///
    /// The `sequence` field is set to 0 here; the storage backend assigns the
    /// real sequence number in [`ExecutionJournal::write`].
    pub fn new(run_id: Uuid, root_run_id: Uuid, event: JournalEvent) -> Self {
        Self {
            run_id,
            root_run_id,
            sequence: SequenceNumber::new(0),
            timestamp: Utc::now(),
            event,
        }
    }
}

/// Information about a task being started.
///
/// Records the item, step, execution-level attempt number (1-based), and
/// the task_id used for result delivery via the `TaskRegistry`.
///
/// The task_id is journalled so that on recovery, the same ID can be
/// re-registered in the `TaskRegistry`. This allows a worker that completed
/// the task before the crash to deliver its result via CompleteTask retry.
///
/// This is the attempt count across orchestrator crashes/recoveries, distinct
/// from the transport-level retry counter in the plugin layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAttempt {
    /// The item index within the run.
    pub item_index: u32,
    /// The step index within the workflow.
    pub step_index: usize,
    /// The execution attempt number (1-based).
    pub attempt: u32,
    /// Unique task ID for TaskRegistry result delivery.
    ///
    /// Defaults to empty string for backward compatibility with journals
    /// written before task_id journalling was introduced.
    #[serde(default)]
    pub task_id: String,
}

impl TaskAttempt {
    /// Create a new task attempt record.
    pub fn new(item_index: u32, step_index: usize, attempt: u32, task_id: String) -> Self {
        Self {
            item_index,
            step_index,
            attempt,
            task_id,
        }
    }
}

/// Group of task attempts belonging to a single run within a `TasksStarted` event.
///
/// When a scheduling round contains tasks from multiple runs (parent and subflows),
/// they are grouped by run_id within a single `TasksStarted` event rather than
/// requiring separate events per run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTaskAttempts {
    /// The run these tasks belong to.
    pub run_id: Uuid,
    /// Tasks being started for this run.
    pub tasks: Vec<TaskAttempt>,
}

/// Execution events recorded in the journal.
///
/// These events capture the state transitions during workflow execution
/// and can be replayed to reconstruct RunState during recovery.
///
/// Each event carries its own `run_id` identifying which specific run
/// (parent or subflow) it belongs to. This allows a single journal
/// (keyed by `root_run_id`) to contain events from the entire execution
/// tree without needing an external envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JournalEvent {
    // =========================================================================
    // Run Lifecycle
    // =========================================================================
    /// Root run created with initial configuration.
    RootRunCreated {
        /// The run this event belongs to.
        run_id: Uuid,
        /// The flow being executed.
        flow_id: BlobId,
        /// Input data for each item.
        inputs: Vec<ValueRef>,
        /// Variables provided for execution.
        variables: HashMap<String, ValueRef>,
    },

    /// Steps needed for a specific item after analysis.
    ///
    /// Emitted per-item after flow analysis (initial step discovery) and again
    /// whenever the needed step set changes (e.g., conditional branches resolved).
    /// Each item gets its own event because conditional output expressions can
    /// cause different items to need different steps based on their input.
    StepsNeeded {
        /// The run this event belongs to.
        run_id: Uuid,
        /// The item index this event applies to.
        item_index: u32,
        /// Step indices that are needed.
        step_indices: Vec<usize>,
    },

    /// Run completed (terminal state).
    RunCompleted {
        /// The run this event belongs to.
        run_id: Uuid,
        /// Final execution status.
        status: ExecutionStatus,
    },

    // =========================================================================
    // Task Lifecycle
    // =========================================================================
    /// Tasks started in a scheduling round.
    ///
    /// Written before spawning task futures. Records the execution attempt
    /// number for each task, enabling attempt tracking across crashes. A
    /// `TasksStarted` without a matching `TaskCompleted` indicates the task
    /// was in-flight when the crash occurred.
    ///
    /// Tasks are grouped by run_id via [`RunTaskAttempts`], allowing a single
    /// event to span multiple runs in the execution tree.
    TasksStarted {
        /// Task attempts grouped by run.
        runs: Vec<RunTaskAttempts>,
    },

    /// Task completed with result.
    TaskCompleted {
        /// The run this event belongs to.
        run_id: Uuid,
        /// The item index within the run.
        item_index: u32,
        /// The step index within the workflow.
        step_index: usize,
        /// The execution result (success or failure).
        result: FlowResult,
    },

    // =========================================================================
    // Dependency State
    // =========================================================================
    /// Steps became unblocked (dependencies satisfied).
    StepsUnblocked {
        /// The run this event belongs to.
        run_id: Uuid,
        /// The item index within the run.
        item_index: u32,
        /// Step indices that are now ready to execute.
        step_indices: Vec<usize>,
    },

    // =========================================================================
    // Item Lifecycle
    // =========================================================================
    /// Individual item completed.
    ItemCompleted {
        /// The run this event belongs to.
        run_id: Uuid,
        /// The item index within the run.
        item_index: u32,
        /// The item result.
        result: FlowResult,
    },

    // =========================================================================
    // Subflow Lifecycle
    // =========================================================================
    /// A sub-run was created and associated with its parent task.
    ///
    /// Combines the sub-run's creation data (flow_id, inputs, variables) with
    /// the parent task association (item_index, step_index, subflow_key).
    /// During recovery, this single event provides everything needed to both
    /// reconstruct the sub-run's RunState AND populate the dedup map.
    SubRunCreated {
        /// The sub-run's run ID.
        run_id: Uuid,
        /// The flow being executed by the sub-run.
        flow_id: BlobId,
        /// Input data for each item in the sub-run.
        inputs: Vec<ValueRef>,
        /// Variables provided for the sub-run execution.
        variables: HashMap<String, ValueRef>,
        /// The parent run that created this sub-run.
        parent_run_id: Uuid,
        /// Item index within the parent run.
        item_index: u32,
        /// Step index within the parent run.
        step_index: usize,
        /// Caller-provided deduplication key.
        subflow_key: Uuid,
    },
}

impl JournalEvent {
    /// Returns the run IDs whose `RunState` is modified by this event.
    ///
    /// Only events that mutate `RunState` are included: `StepsNeeded`,
    /// `TasksStarted`, `TaskCompleted`. All others are either informational
    /// or handled separately by the recovery loop.
    pub fn affected_run_ids(&self) -> AffectedRunIds<'_> {
        match self {
            JournalEvent::StepsNeeded { run_id, .. }
            | JournalEvent::TaskCompleted { run_id, .. } => AffectedRunIds::One(Some(run_id)),
            JournalEvent::TasksStarted { runs } => AffectedRunIds::TaskAttempts(runs.iter()),
            _ => AffectedRunIds::None,
        }
    }
}

/// Zero-allocation iterator over run IDs affected by a journal event.
pub enum AffectedRunIds<'a> {
    /// No runs affected (informational events).
    None,
    /// Exactly one run affected (most events).
    One(Option<&'a Uuid>),
    /// Runs from a `TasksStarted` event spanning multiple runs.
    TaskAttempts(std::slice::Iter<'a, RunTaskAttempts>),
}

impl<'a> Iterator for AffectedRunIds<'a> {
    type Item = &'a Uuid;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            AffectedRunIds::None => Option::None,
            AffectedRunIds::One(id) => id.take(),
            AffectedRunIds::TaskAttempts(iter) => iter.next().map(|r| &r.run_id),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            AffectedRunIds::None => (0, Some(0)),
            AffectedRunIds::One(Some(_)) => (1, Some(1)),
            AffectedRunIds::One(Option::None) => (0, Some(0)),
            AffectedRunIds::TaskAttempts(iter) => iter.size_hint(),
        }
    }
}

/// Trait for write-ahead journalling of execution events.
///
/// This trait provides the foundation for recording execution events that
/// can be replayed to recover run state after a crash. Implementations
/// should ensure durability guarantees appropriate for the backend.
///
/// Journals are keyed by `root_run_id`, so all events for an execution tree
/// (parent + subflows) share a single journal with a unified sequence space.
/// Each event carries its own `run_id` identifying which specific run it
/// belongs to.
///
/// # Durability
///
/// Each [`write`](Self::write) call durably persists the event before returning.
/// After `write` returns, the event is guaranteed to survive process crashes
/// or restarts.
pub trait ExecutionJournal: Send + Sync {
    /// Initialize the journal backend (e.g., create tables, set up schema).
    ///
    /// Called by the configuration layer after the journal is created and before
    /// it is used. The default implementation is a no-op, suitable for backends
    /// that require no initialization (e.g., in-memory or no-op journals).
    fn initialize_journal(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async { Ok(()) }.boxed()
    }

    /// Write a journal event durably.
    ///
    /// The event is persisted to the journal for the given `root_run_id`.
    /// Sequence numbers are monotonically increasing within each root journal.
    /// The implementation assigns a timestamp internally.
    ///
    /// This method guarantees durability — after it returns, the event will
    /// survive process crashes or restarts.
    ///
    /// # Arguments
    /// * `root_run_id` - The root run's journal to write to
    /// * `event` - The journal event to write
    ///
    /// # Returns
    /// The sequence number assigned to this event
    fn write(
        &self,
        root_run_id: Uuid,
        event: JournalEvent,
    ) -> BoxFuture<'_, error_stack::Result<SequenceNumber, StateError>>;

    /// Stream journal events for a root run starting from a sequence number.
    ///
    /// Returns a stream yielding all events in the journal (across all runs
    /// in the tree) in sequence order, starting from `from_sequence`.
    ///
    /// # Arguments
    /// * `root_run_id` - The root run's journal to read from
    /// * `from_sequence` - Start reading from this sequence (inclusive)
    ///
    /// # Returns
    /// A stream of events in sequence order
    fn stream_from(
        &self,
        root_run_id: Uuid,
        from_sequence: SequenceNumber,
    ) -> JournalEventStream<'_>;

    /// Get the latest sequence number for a root run's journal.
    ///
    /// # Arguments
    /// * `root_run_id` - The root run to query
    ///
    /// # Returns
    /// The latest sequence number, or None if no entries exist
    fn latest_sequence(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<SequenceNumber>, StateError>>;

    /// List root runs with journal entries (for recovery).
    ///
    /// This returns information about all root run journals,
    /// which is used during startup to identify execution trees that need recovery.
    ///
    /// # Returns
    /// Information about each root run's journal
    fn list_active_roots(
        &self,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RootJournalInfo>, StateError>>;

    /// Follow a root run's journal, yielding events as they arrive.
    ///
    /// Like [`stream_from`](Self::stream_from), but never terminates — when all
    /// existing events have been yielded, it polls for new entries. The stream
    /// should be dropped by the consumer when it is no longer needed (e.g., after
    /// receiving a `RunCompleted` event).
    ///
    /// The default implementation polls `stream_from` in a loop with a 500ms
    /// delay between batches. Backends with native change-notification support
    /// (e.g., NATS JetStream) should override this with a push-based implementation.
    ///
    /// # Arguments
    /// * `root_run_id` - The root run's journal to follow
    /// * `from_sequence` - Start reading from this sequence (inclusive)
    fn follow(&self, root_run_id: Uuid, from_sequence: SequenceNumber) -> JournalEventStream<'_> {
        Box::pin(async_stream::stream! {
            let mut cursor = from_sequence;
            loop {
                let mut got_any = false;
                let mut batch = self.stream_from(root_run_id, cursor);
                while let Some(item) = futures::StreamExt::next(&mut batch).await {
                    match item {
                        Ok(entry) => {
                            cursor = entry.sequence.next();
                            got_any = true;
                            yield Ok(entry);
                        }
                        Err(e) => {
                            yield Err(e);
                            return;
                        }
                    }
                }
                drop(batch);
                if !got_any {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        })
    }
}
