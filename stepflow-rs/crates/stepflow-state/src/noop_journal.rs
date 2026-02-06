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
/// # Example
///
/// ```rust
/// use stepflow_state::{ExecutionJournal, NoOpJournal};
///
/// let journal = NoOpJournal::new();
/// // Journal operations will succeed but nothing is persisted
/// ```
#[derive(Debug, Clone, Default)]
pub struct NoOpJournal;

impl NoOpJournal {
    /// Create a new no-op journal.
    pub fn new() -> Self {
        Self
    }
}

impl ExecutionJournal for NoOpJournal {
    fn append(
        &self,
        _entry: JournalEntry,
    ) -> BoxFuture<'_, error_stack::Result<SequenceNumber, StateError>> {
        // Return a dummy sequence number; nothing is actually stored
        async move { Ok(SequenceNumber::new(0)) }.boxed()
    }

    fn read_from(
        &self,
        _root_run_id: Uuid,
        _from_sequence: SequenceNumber,
        _limit: usize,
    ) -> BoxFuture<'_, error_stack::Result<Vec<(SequenceNumber, JournalEntry)>, StateError>> {
        // No entries to read
        async move { Ok(Vec::new()) }.boxed()
    }

    fn latest_sequence(
        &self,
        _root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<SequenceNumber>, StateError>> {
        // No entries, so no latest sequence
        async move { Ok(None) }.boxed()
    }

    fn list_active_roots(
        &self,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RootJournalInfo>, StateError>> {
        // No active roots
        async move { Ok(Vec::new()) }.boxed()
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

        let entry = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::RunCompleted {
                status: ExecutionStatus::Completed,
            },
        );

        let result = journal.append(entry).await.unwrap();
        assert_eq!(result, SequenceNumber::new(0));
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
        let journal = NoOpJournal::new();
        let run_id = Uuid::now_v7();

        let latest = journal.latest_sequence(run_id).await.unwrap();
        assert!(latest.is_none());
    }

    #[tokio::test]
    async fn test_noop_list_active_roots() {
        let journal = NoOpJournal::new();

        let roots = journal.list_active_roots().await.unwrap();
        assert!(roots.is_empty());
    }
}
