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
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::ValueRef;
use stepflow_core::{BlobId, FlowResult};
use uuid::Uuid;

use crate::StateError;

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
    /// When this event occurred.
    pub timestamp: DateTime<Utc>,
    /// The execution event.
    pub event: JournalEvent,
}

impl JournalEntry {
    /// Create a new journal entry with the current timestamp.
    pub fn new(run_id: Uuid, root_run_id: Uuid, event: JournalEvent) -> Self {
        Self {
            run_id,
            root_run_id,
            timestamp: Utc::now(),
            event,
        }
    }
}

/// Per-item step indices for batch initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSteps {
    /// The item index within the run.
    pub item_index: u32,
    /// Step indices that are needed for this item.
    pub step_indices: Vec<usize>,
}

/// Information about a task being started.
///
/// Records the item, step, and execution-level attempt number (1-based).
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
}

impl TaskAttempt {
    /// Create a new task attempt record.
    pub fn new(item_index: u32, step_index: usize, attempt: u32) -> Self {
        Self {
            item_index,
            step_index,
            attempt,
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
    /// Run created with initial configuration.
    RunCreated {
        /// The run this event belongs to.
        run_id: Uuid,
        /// The flow being executed.
        flow_id: BlobId,
        /// Input data for each item.
        inputs: Vec<ValueRef>,
        /// Variables provided for execution.
        variables: HashMap<String, ValueRef>,
        /// Parent run ID (for subflows).
        parent_run_id: Option<Uuid>,
    },

    /// Run initialized after step discovery.
    RunInitialized {
        /// The run this event belongs to.
        run_id: Uuid,
        /// Per-item needed step indices.
        needed_steps: Vec<ItemSteps>,
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
    /// A subflow was submitted by a parent step.
    ///
    /// Records the association between a parent task `(parent_run_id, item_index,
    /// step_index, subflow_key)` and the `subflow_run_id` it created. During
    /// recovery, this mapping allows the executor to match re-submitted subflows
    /// to their pre-crash counterparts, avoiding duplicate execution.
    SubflowSubmitted {
        /// The parent run that submitted this subflow.
        parent_run_id: Uuid,
        /// Item index within the parent run.
        item_index: u32,
        /// Step index within the parent run.
        step_index: usize,
        /// Caller-provided deduplication key.
        subflow_key: Uuid,
        /// The run ID assigned to the subflow.
        subflow_run_id: Uuid,
    },
}

impl JournalEvent {
    /// Check whether this event involves a specific run.
    ///
    /// For most events this is a simple run_id comparison. For `TasksStarted`,
    /// which groups tasks by run, it checks all contained run groups.
    pub fn involves_run(&self, target: Uuid) -> bool {
        match self {
            JournalEvent::RunCreated { run_id, .. }
            | JournalEvent::RunInitialized { run_id, .. }
            | JournalEvent::RunCompleted { run_id, .. }
            | JournalEvent::TaskCompleted { run_id, .. }
            | JournalEvent::StepsUnblocked { run_id, .. }
            | JournalEvent::ItemCompleted { run_id, .. } => *run_id == target,
            JournalEvent::TasksStarted { runs } => runs.iter().any(|r| r.run_id == target),
            JournalEvent::SubflowSubmitted {
                parent_run_id, ..
            } => *parent_run_id == target,
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

    /// Read journal events for a root run starting from a sequence number.
    ///
    /// Returns all events in the journal (across all runs in the tree).
    /// Use [`JournalEvent::involves_run`] to filter for a specific run.
    ///
    /// # Arguments
    /// * `root_run_id` - The root run's journal to read from
    /// * `from_sequence` - Start reading from this sequence (inclusive)
    /// * `limit` - Maximum number of events to return
    ///
    /// # Returns
    /// Events in sequence order
    fn read_from(
        &self,
        root_run_id: Uuid,
        from_sequence: SequenceNumber,
        limit: usize,
    ) -> BoxFuture<'_, error_stack::Result<Vec<JournalEvent>, StateError>>;

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
}
