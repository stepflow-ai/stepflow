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

//! No-op execution journal for deployments without persistence.

use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use futures::FutureExt as _;
use futures::future::BoxFuture;
use uuid::Uuid;

use crate::{ExecutionJournal, JournalEntry, RootJournalInfo, SequenceNumber, StateError};

/// A no-op execution journal that discards all entries.
///
/// This implementation is suitable for deployments where durability is not
/// required (e.g., development, testing, or ephemeral workloads). All append
/// operations succeed immediately but entries are not persisted, so recovery
/// is not possible.
///
/// Sequence numbers are tracked per root_run_id to maintain the trait contract
/// of returning monotonically increasing sequence numbers, even though the
/// entries themselves are not stored.
///
/// # Example
///
/// ```rust
/// use stepflow_state::{ExecutionJournal, NoOpJournal};
///
/// let journal = NoOpJournal::new();
/// // Journal operations will succeed but nothing is persisted
/// ```
#[derive(Debug, Default)]
pub struct NoOpJournal {
    /// Track sequence numbers per root_run_id for consistency.
    sequences: DashMap<Uuid, AtomicU64>,
}

impl NoOpJournal {
    /// Create a new no-op journal.
    pub fn new() -> Self {
        Self {
            sequences: DashMap::new(),
        }
    }
}

impl ExecutionJournal for NoOpJournal {
    fn append(
        &self,
        entry: JournalEntry,
    ) -> BoxFuture<'_, error_stack::Result<SequenceNumber, StateError>> {
        // Track sequence numbers per root_run_id for trait contract consistency.
        // Entry is not stored, but sequence numbers are monotonically increasing.
        let root_run_id = entry.root_run_id;
        let seq = self
            .sequences
            .entry(root_run_id)
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
        async move { Ok(SequenceNumber::new(seq)) }.boxed()
    }

    fn flush(&self, _root_run_id: Uuid) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        // Nothing to flush - no-op journal doesn't persist anything
        async move { Ok(()) }.boxed()
    }

    fn read_from(
        &self,
        _root_run_id: Uuid,
        _from_sequence: SequenceNumber,
        _limit: usize,
    ) -> BoxFuture<'_, error_stack::Result<Vec<(SequenceNumber, JournalEntry)>, StateError>> {
        // No entries to read - entries are not stored
        async move { Ok(Vec::new()) }.boxed()
    }

    fn latest_sequence(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<SequenceNumber>, StateError>> {
        // Return the latest sequence number if any appends have been made
        let seq = self.sequences.get(&root_run_id).map(|entry| {
            let current = entry.load(Ordering::Relaxed);
            if current == 0 {
                None
            } else {
                Some(SequenceNumber::new(current - 1))
            }
        });
        async move { Ok(seq.flatten()) }.boxed()
    }

    fn list_active_roots(
        &self,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RootJournalInfo>, StateError>> {
        // Return tracked roots (though entries are not persisted)
        let roots: Vec<RootJournalInfo> = self
            .sequences
            .iter()
            .map(|entry| {
                let root_run_id = *entry.key();
                let latest = entry.value().load(Ordering::Relaxed);
                RootJournalInfo {
                    root_run_id,
                    latest_sequence: SequenceNumber::new(latest.saturating_sub(1)),
                    entry_count: latest,
                }
            })
            .filter(|info| info.entry_count > 0)
            .collect();
        async move { Ok(roots) }.boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_append() {
        use crate::JournalEvent;
        use stepflow_core::status::ExecutionStatus;

        let journal = NoOpJournal::new();
        let run_id = Uuid::now_v7();

        // First append should return sequence 0
        let entry1 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunCompleted {
                status: ExecutionStatus::Completed,
            },
        );
        let result1 = journal.append(entry1).await.unwrap();
        assert_eq!(result1, SequenceNumber::new(0));

        // Second append should return sequence 1 (incrementing)
        let entry2 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunCompleted {
                status: ExecutionStatus::Completed,
            },
        );
        let result2 = journal.append(entry2).await.unwrap();
        assert_eq!(result2, SequenceNumber::new(1));

        // Different root_run_id should start at 0
        let run_id2 = Uuid::now_v7();
        let entry3 = JournalEntry::new(
            run_id2,
            run_id2,
            JournalEvent::RunCompleted {
                status: ExecutionStatus::Completed,
            },
        );
        let result3 = journal.append(entry3).await.unwrap();
        assert_eq!(result3, SequenceNumber::new(0));
    }

    #[tokio::test]
    async fn test_noop_read_from() {
        let journal = NoOpJournal::new();
        let run_id = Uuid::now_v7();

        let entries = journal
            .read_from(run_id, SequenceNumber::new(0), 100)
            .await
            .unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_noop_latest_sequence() {
        use crate::JournalEvent;
        use stepflow_core::status::ExecutionStatus;

        let journal = NoOpJournal::new();
        let run_id = Uuid::now_v7();

        // No appends yet - should be None
        let latest = journal.latest_sequence(run_id).await.unwrap();
        assert!(latest.is_none());

        // After appending, should return the latest sequence
        let entry = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunCompleted {
                status: ExecutionStatus::Completed,
            },
        );
        journal.append(entry).await.unwrap();

        let latest = journal.latest_sequence(run_id).await.unwrap();
        assert_eq!(latest, Some(SequenceNumber::new(0)));

        // After second append
        let entry2 = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunCompleted {
                status: ExecutionStatus::Completed,
            },
        );
        journal.append(entry2).await.unwrap();

        let latest = journal.latest_sequence(run_id).await.unwrap();
        assert_eq!(latest, Some(SequenceNumber::new(1)));
    }

    #[tokio::test]
    async fn test_noop_list_active_roots() {
        let journal = NoOpJournal::new();

        let roots = journal.list_active_roots().await.unwrap();
        assert!(roots.is_empty());
    }
}
