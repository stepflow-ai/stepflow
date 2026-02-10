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

/// Execution events recorded in the journal.
///
/// These events capture the state transitions during workflow execution
/// and can be replayed to reconstruct RunState during recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JournalEvent {
    // =========================================================================
    // Run Lifecycle
    // =========================================================================
    /// Run created with initial configuration.
    RunCreated {
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
        /// Per-item needed step indices.
        needed_steps: Vec<ItemSteps>,
    },

    /// Run completed (terminal state).
    RunCompleted {
        /// Final execution status.
        status: ExecutionStatus,
    },

    // =========================================================================
    // Task Lifecycle
    // =========================================================================
    /// Task completed with result.
    TaskCompleted {
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
        /// The item index within the run.
        item_index: u32,
        /// The item result.
        result: FlowResult,
    },
    // Note: Subflow events are not needed as a separate category.
    // Subflows are tracked via their own RunCreated events with parent_run_id set.
    // Since all events for an execution tree share the same journal (keyed by root_run_id),
    // the parent-child relationship is implicit in the journal structure.
}

/// Trait for write-ahead journalling of execution events.
///
/// This trait provides the foundation for recording execution events that
/// can be replayed to recover run state after a crash. Implementations
/// should ensure durability guarantees appropriate for the backend.
///
/// Journals are keyed by `root_run_id`, so all events for an execution tree
/// (parent + subflows) share a single journal with a unified sequence space.
///
/// # Buffering and Durability
///
/// The journal supports a two-phase commit model:
///
/// 1. **Append**: Records events to an internal buffer. This is fast but may not
///    be durable - if the process crashes before flush, buffered events may be lost.
///
/// 2. **Flush**: Ensures all buffered events are durably committed. This acts as
///    a barrier - after flush returns, all previously appended events are guaranteed
///    to survive crashes.
///
/// This model allows high-throughput event recording while providing explicit
/// durability points before operations with side effects.
pub trait ExecutionJournal: Send + Sync {
    /// Append a journal entry to the buffer.
    ///
    /// The entry is added to the journal buffer for `entry.root_run_id`.
    /// Sequence numbers are monotonically increasing within each root journal.
    ///
    /// **Note**: This method may buffer the entry without immediately persisting it.
    /// Call [`flush`](Self::flush) to ensure all buffered entries are durably committed.
    ///
    /// # Arguments
    /// * `entry` - The journal entry to append
    ///
    /// # Returns
    /// The sequence number assigned to this entry
    fn append(
        &self,
        entry: JournalEntry,
    ) -> BoxFuture<'_, error_stack::Result<SequenceNumber, StateError>>;

    /// Flush all buffered entries to durable storage.
    ///
    /// This method acts as a durability barrier - after it returns successfully,
    /// all previously appended entries are guaranteed to be persisted and will
    /// survive process crashes or restarts.
    ///
    /// Call this before executing steps with side effects to ensure that prior
    /// step results are durably recorded. This is important because:
    ///
    /// - Steps may have non-deterministic outputs
    /// - Steps may have external side effects (API calls, database writes)
    /// - On recovery, we need the exact results from the original execution
    ///
    /// # Arguments
    /// * `root_run_id` - The root run whose journal should be flushed
    fn flush(&self, root_run_id: Uuid) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Read journal entries for a root run starting from a sequence number.
    ///
    /// Returns all entries in the journal (across all runs in the tree).
    /// Use the `run_id` field in each entry to filter for a specific run.
    ///
    /// # Arguments
    /// * `root_run_id` - The root run's journal to read from
    /// * `from_sequence` - Start reading from this sequence (inclusive)
    /// * `limit` - Maximum number of entries to return
    ///
    /// # Returns
    /// A vector of (sequence_number, entry) pairs in sequence order
    fn read_from(
        &self,
        root_run_id: Uuid,
        from_sequence: SequenceNumber,
        limit: usize,
    ) -> BoxFuture<'_, error_stack::Result<Vec<(SequenceNumber, JournalEntry)>, StateError>>;

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
