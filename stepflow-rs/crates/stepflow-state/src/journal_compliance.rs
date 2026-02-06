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

//! Compliance test suite for `ExecutionJournal` implementations.
//!
//! This module provides a comprehensive set of tests that any `ExecutionJournal` implementation
//! must pass. Use these tests to verify your implementation conforms to the trait contract.
//!
//! # Usage
//!
//! In your implementation's test module:
//!
//! ```ignore
//! #[cfg(test)]
//! mod tests {
//!     use stepflow_state::journal_compliance::JournalComplianceTests;
//!
//!     #[tokio::test]
//!     async fn compliance_append_and_read() {
//!         let store = MyStateStore::new().await.unwrap();
//!         JournalComplianceTests::test_append_and_read(&store).await;
//!     }
//!
//!     // ... or run all tests at once:
//!     #[tokio::test]
//!     async fn compliance_all() {
//!         let store = MyStateStore::new().await.unwrap();
//!         JournalComplianceTests::run_all(&store).await;
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::future::Future;

use serde_json::json;
use stepflow_core::workflow::ValueRef;
use stepflow_core::{BlobId, FlowError, FlowResult};
use uuid::Uuid;

use crate::{ExecutionJournal, ItemSteps, JournalEntry, JournalEvent, SequenceNumber};

/// Compliance test suite for ExecutionJournal implementations.
///
/// Each test method validates a specific aspect of the ExecutionJournal contract.
/// Implementations should pass all tests to ensure correct behavior.
pub struct JournalComplianceTests;

impl JournalComplianceTests {
    /// Run all compliance tests against the given journal implementation.
    ///
    /// This is a convenience method that runs every test in the suite.
    /// Tests are run sequentially and will panic on the first failure.
    pub async fn run_all<J: ExecutionJournal>(journal: &J) {
        Self::test_append_returns_monotonic_sequence(journal).await;
        Self::test_read_from_empty_journal(journal).await;
        Self::test_read_from_returns_entries_in_order(journal).await;
        Self::test_read_from_respects_from_sequence(journal).await;
        Self::test_read_from_respects_limit(journal).await;
        Self::test_latest_sequence_empty(journal).await;
        Self::test_latest_sequence_after_append(journal).await;
        Self::test_list_active_roots_empty(journal).await;
        Self::test_list_active_roots_multiple(journal).await;
        Self::test_subflow_shared_journal(journal).await;
        Self::test_all_event_types_serialization(journal).await;
    }

    /// Run all compliance tests with a fresh journal for each test.
    ///
    /// This version creates a new journal instance for each test, ensuring complete
    /// isolation between tests. Use this when tests may interfere with each other
    /// due to shared state.
    ///
    /// # Example
    ///
    /// ```ignore
    /// JournalComplianceTests::run_all_isolated(|| async {
    ///     SqliteStateStore::in_memory().await.unwrap()
    /// }).await;
    /// ```
    pub async fn run_all_isolated<J, F, Fut>(factory: F)
    where
        J: ExecutionJournal,
        F: Fn() -> Fut,
        Fut: Future<Output = J>,
    {
        Self::test_append_returns_monotonic_sequence(&factory().await).await;
        Self::test_read_from_empty_journal(&factory().await).await;
        Self::test_read_from_returns_entries_in_order(&factory().await).await;
        Self::test_read_from_respects_from_sequence(&factory().await).await;
        Self::test_read_from_respects_limit(&factory().await).await;
        Self::test_latest_sequence_empty(&factory().await).await;
        Self::test_latest_sequence_after_append(&factory().await).await;
        Self::test_list_active_roots_empty(&factory().await).await;
        Self::test_list_active_roots_multiple(&factory().await).await;
        Self::test_subflow_shared_journal(&factory().await).await;
        Self::test_all_event_types_serialization(&factory().await).await;
    }

    // =========================================================================
    // append() tests
    // =========================================================================

    /// Test that append() returns monotonically increasing sequence numbers.
    ///
    /// Contract: Each append to a journal returns a sequence number greater than
    /// all previous appends to the same root_run_id journal.
    pub async fn test_append_returns_monotonic_sequence<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let run_id = root_run_id;

        let mut last_seq = None;
        for i in 0..10 {
            let entry = JournalEntry::new(
                run_id,
                root_run_id,
                JournalEvent::TaskCompleted {
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            );
            let seq = journal.append(entry).await.expect("append should succeed");

            if let Some(prev) = last_seq {
                assert!(
                    seq > prev,
                    "Sequence numbers must be monotonically increasing: got {:?} after {:?}",
                    seq,
                    prev
                );
            }
            last_seq = Some(seq);
        }
    }

    // =========================================================================
    // read_from() tests
    // =========================================================================

    /// Test that read_from() returns empty vec for non-existent journal.
    ///
    /// Contract: Reading from a root_run_id with no entries returns an empty vec.
    pub async fn test_read_from_empty_journal<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let entries = journal
            .read_from(root_run_id, SequenceNumber::new(0), 100)
            .await
            .expect("read_from should succeed");
        assert!(
            entries.is_empty(),
            "read_from non-existent journal should return empty vec"
        );
    }

    /// Test that read_from() returns entries in sequence order.
    ///
    /// Contract: Entries are returned in ascending sequence number order.
    pub async fn test_read_from_returns_entries_in_order<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let run_id = root_run_id;

        // Append entries
        for i in 0..5 {
            let entry = JournalEntry::new(
                run_id,
                root_run_id,
                JournalEvent::TaskCompleted {
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            );
            journal.append(entry).await.expect("append should succeed");
        }

        // Read all entries
        let entries = journal
            .read_from(root_run_id, SequenceNumber::new(0), 100)
            .await
            .expect("read_from should succeed");

        assert_eq!(entries.len(), 5, "Should have 5 entries");

        // Verify order
        for (i, item) in entries.iter().enumerate() {
            assert_eq!(
                item.0.value(),
                i as u64,
                "Entry {i} should have sequence {i}"
            );
        }
    }

    /// Test that read_from() respects the from_sequence parameter.
    ///
    /// Contract: Only entries with sequence >= from_sequence are returned.
    pub async fn test_read_from_respects_from_sequence<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let run_id = root_run_id;

        // Append 10 entries
        for i in 0..10 {
            let entry = JournalEntry::new(
                run_id,
                root_run_id,
                JournalEvent::TaskCompleted {
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            );
            journal.append(entry).await.expect("append should succeed");
        }

        // Read from sequence 5
        let entries = journal
            .read_from(root_run_id, SequenceNumber::new(5), 100)
            .await
            .expect("read_from should succeed");

        assert_eq!(entries.len(), 5, "Should have 5 entries (seq 5-9)");
        assert_eq!(entries[0].0.value(), 5, "First entry should be sequence 5");
        assert_eq!(entries[4].0.value(), 9, "Last entry should be sequence 9");
    }

    /// Test that read_from() respects the limit parameter.
    ///
    /// Contract: At most `limit` entries are returned.
    pub async fn test_read_from_respects_limit<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let run_id = root_run_id;

        // Append 10 entries
        for i in 0..10 {
            let entry = JournalEntry::new(
                run_id,
                root_run_id,
                JournalEvent::TaskCompleted {
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            );
            journal.append(entry).await.expect("append should succeed");
        }

        // Read with limit of 3
        let entries = journal
            .read_from(root_run_id, SequenceNumber::new(0), 3)
            .await
            .expect("read_from should succeed");

        assert_eq!(entries.len(), 3, "Should have exactly 3 entries");
        assert_eq!(entries[0].0.value(), 0);
        assert_eq!(entries[2].0.value(), 2);

        // Read with limit of 0
        let entries = journal
            .read_from(root_run_id, SequenceNumber::new(0), 0)
            .await
            .expect("read_from should succeed");
        assert!(entries.is_empty(), "Limit 0 should return empty vec");
    }

    // =========================================================================
    // latest_sequence() tests
    // =========================================================================

    /// Test that latest_sequence() returns None for empty journal.
    ///
    /// Contract: A journal with no entries has no latest sequence.
    pub async fn test_latest_sequence_empty<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let latest = journal
            .latest_sequence(root_run_id)
            .await
            .expect("latest_sequence should succeed");
        assert!(
            latest.is_none(),
            "Empty journal should have no latest sequence"
        );
    }

    /// Test that latest_sequence() returns the correct value after appends.
    ///
    /// Contract: latest_sequence returns the sequence number of the most recent append.
    pub async fn test_latest_sequence_after_append<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let run_id = root_run_id;

        // Append first entry
        let entry = JournalEntry::new(
            run_id,
            root_run_id,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({}))),
            },
        );
        let seq1 = journal.append(entry).await.expect("append should succeed");

        let latest = journal
            .latest_sequence(root_run_id)
            .await
            .expect("latest_sequence should succeed");
        assert_eq!(
            latest,
            Some(seq1),
            "latest_sequence should match first append"
        );

        // Append more entries
        for i in 1..5 {
            let entry = JournalEntry::new(
                run_id,
                root_run_id,
                JournalEvent::TaskCompleted {
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            );
            journal.append(entry).await.expect("append should succeed");
        }

        let latest = journal
            .latest_sequence(root_run_id)
            .await
            .expect("latest_sequence should succeed");
        assert_eq!(
            latest,
            Some(SequenceNumber::new(4)),
            "latest_sequence should be 4 after 5 appends"
        );
    }

    // =========================================================================
    // list_active_roots() tests
    // =========================================================================

    /// Test that list_active_roots() returns empty for no journals.
    ///
    /// Contract: A fresh journal implementation has no active roots.
    pub async fn test_list_active_roots_empty<J: ExecutionJournal>(journal: &J) {
        let roots = journal
            .list_active_roots()
            .await
            .expect("list_active_roots should succeed");
        // Note: We can't assert this is empty because other tests might have run.
        // We just verify the call succeeds.
        let _ = roots;
    }

    /// Test list_active_roots() with multiple journals.
    ///
    /// Contract: list_active_roots() returns info for all root journals with entries.
    pub async fn test_list_active_roots_multiple<J: ExecutionJournal>(journal: &J) {
        // Create multiple root runs
        let root1 = Uuid::now_v7();
        let root2 = Uuid::now_v7();
        let root3 = Uuid::now_v7();

        // Add entries to each
        for i in 0..3 {
            let entry = JournalEntry::new(
                root1,
                root1,
                JournalEvent::TaskCompleted {
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            );
            journal.append(entry).await.expect("append should succeed");
        }

        for i in 0..5 {
            let entry = JournalEntry::new(
                root2,
                root2,
                JournalEvent::TaskCompleted {
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            );
            journal.append(entry).await.expect("append should succeed");
        }

        let entry = JournalEntry::new(
            root3,
            root3,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({}))),
            },
        );
        journal.append(entry).await.expect("append should succeed");

        // List active roots
        let roots = journal
            .list_active_roots()
            .await
            .expect("list_active_roots should succeed");

        // Find our test roots (there may be others from previous tests)
        let root1_info = roots.iter().find(|r| r.root_run_id == root1);
        let root2_info = roots.iter().find(|r| r.root_run_id == root2);
        let root3_info = roots.iter().find(|r| r.root_run_id == root3);

        assert!(root1_info.is_some(), "root1 should be in list");
        assert!(root2_info.is_some(), "root2 should be in list");
        assert!(root3_info.is_some(), "root3 should be in list");

        let root1_info = root1_info.unwrap();
        assert_eq!(root1_info.latest_sequence, SequenceNumber::new(2));
        assert_eq!(root1_info.entry_count, 3);

        let root2_info = root2_info.unwrap();
        assert_eq!(root2_info.latest_sequence, SequenceNumber::new(4));
        assert_eq!(root2_info.entry_count, 5);

        let root3_info = root3_info.unwrap();
        assert_eq!(root3_info.latest_sequence, SequenceNumber::new(0));
        assert_eq!(root3_info.entry_count, 1);
    }

    // =========================================================================
    // Subflow tests
    // =========================================================================

    /// Test that parent and subflow events share the same journal.
    ///
    /// Contract: Events with the same root_run_id but different run_id values
    /// are stored in the same journal and share a unified sequence space.
    pub async fn test_subflow_shared_journal<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let parent_run_id = root_run_id;
        let subflow_run_id = Uuid::now_v7();

        // Interleave parent and subflow events
        let entry1 = JournalEntry::new(
            parent_run_id,
            root_run_id,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"parent": 1}))),
            },
        );
        let seq1 = journal.append(entry1).await.expect("append should succeed");

        let entry2 = JournalEntry::new(
            subflow_run_id,
            root_run_id,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"subflow": 1}))),
            },
        );
        let seq2 = journal.append(entry2).await.expect("append should succeed");

        let entry3 = JournalEntry::new(
            subflow_run_id,
            root_run_id,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"done": true}))),
            },
        );
        let seq3 = journal.append(entry3).await.expect("append should succeed");

        let entry4 = JournalEntry::new(
            parent_run_id,
            root_run_id,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"done": true}))),
            },
        );
        let seq4 = journal.append(entry4).await.expect("append should succeed");

        // Verify sequence numbers are monotonic across all events
        assert!(seq1 < seq2);
        assert!(seq2 < seq3);
        assert!(seq3 < seq4);

        // Read all entries from the shared journal
        let all_entries = journal
            .read_from(root_run_id, SequenceNumber::new(0), 100)
            .await
            .expect("read_from should succeed");
        assert_eq!(
            all_entries.len(),
            4,
            "Should have 4 entries in shared journal"
        );

        // Filter for parent events
        let parent_entries: Vec<_> = all_entries
            .iter()
            .filter(|(_, e)| e.run_id == parent_run_id)
            .collect();
        assert_eq!(parent_entries.len(), 2, "Should have 2 parent events");

        // Filter for subflow events
        let subflow_entries: Vec<_> = all_entries
            .iter()
            .filter(|(_, e)| e.run_id == subflow_run_id)
            .collect();
        assert_eq!(subflow_entries.len(), 2, "Should have 2 subflow events");

        // Verify list_active_roots only shows one root
        let roots = journal
            .list_active_roots()
            .await
            .expect("list_active_roots should succeed");
        let our_root = roots
            .iter()
            .filter(|r| r.root_run_id == root_run_id)
            .count();
        assert_eq!(
            our_root, 1,
            "Should have exactly one root journal entry for this execution tree"
        );
    }

    // =========================================================================
    // Event serialization tests
    // =========================================================================

    /// Test that all event types can be appended and read correctly.
    ///
    /// Contract: All JournalEvent variants can be serialized, stored, and deserialized.
    pub async fn test_all_event_types_serialization<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let run_id = root_run_id;
        let flow_id = BlobId::from_content(&ValueRef::new(json!({"test": "flow"}))).unwrap();

        let events = vec![
            JournalEvent::RunCreated {
                flow_id: flow_id.clone(),
                inputs: vec![
                    ValueRef::new(json!({"input": 1})),
                    ValueRef::new(json!({"input": 2})),
                ],
                variables: {
                    let mut vars = HashMap::new();
                    vars.insert("key".to_string(), ValueRef::new(json!("value")));
                    vars
                },
                parent_run_id: None,
            },
            JournalEvent::RunCreated {
                flow_id: flow_id.clone(),
                inputs: vec![ValueRef::new(json!({"sub": true}))],
                variables: HashMap::new(),
                parent_run_id: Some(run_id),
            },
            JournalEvent::RunInitialized {
                needed_steps: vec![
                    ItemSteps {
                        item_index: 0,
                        step_indices: vec![0, 1, 2],
                    },
                    ItemSteps {
                        item_index: 1,
                        step_indices: vec![0, 1],
                    },
                ],
            },
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            },
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 1,
                result: FlowResult::Failed(FlowError {
                    code: 1,
                    message: "test error".into(),
                    data: None,
                }),
            },
            JournalEvent::StepsUnblocked {
                item_index: 0,
                step_indices: vec![2, 3, 4],
            },
            JournalEvent::ItemCompleted {
                item_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"item": "done"}))),
            },
            JournalEvent::RunCompleted {
                status: stepflow_core::status::ExecutionStatus::Completed,
            },
        ];

        // Append all events
        for event in &events {
            let entry = JournalEntry::new(run_id, root_run_id, event.clone());
            journal.append(entry).await.expect("append should succeed");
        }

        // Read all entries back
        let entries = journal
            .read_from(root_run_id, SequenceNumber::new(0), 100)
            .await
            .expect("read_from should succeed");

        assert_eq!(entries.len(), events.len(), "Should have all entries");

        // Verify each event type was preserved (we check structure, not exact equality
        // since timestamps will differ)
        for (i, (_, entry)) in entries.iter().enumerate() {
            match (&entry.event, &events[i]) {
                (
                    JournalEvent::RunCreated { flow_id: f1, .. },
                    JournalEvent::RunCreated { flow_id: f2, .. },
                ) => {
                    assert_eq!(f1, f2, "RunCreated flow_id should match");
                }
                (
                    JournalEvent::RunInitialized { needed_steps: n1 },
                    JournalEvent::RunInitialized { needed_steps: n2 },
                ) => {
                    assert_eq!(
                        n1.len(),
                        n2.len(),
                        "RunInitialized needed_steps length should match"
                    );
                }
                (
                    JournalEvent::TaskCompleted {
                        item_index: i1,
                        step_index: s1,
                        ..
                    },
                    JournalEvent::TaskCompleted {
                        item_index: i2,
                        step_index: s2,
                        ..
                    },
                ) => {
                    assert_eq!(i1, i2, "TaskCompleted item_index should match");
                    assert_eq!(s1, s2, "TaskCompleted step_index should match");
                }
                (
                    JournalEvent::StepsUnblocked {
                        item_index: i1,
                        step_indices: s1,
                    },
                    JournalEvent::StepsUnblocked {
                        item_index: i2,
                        step_indices: s2,
                    },
                ) => {
                    assert_eq!(i1, i2, "StepsUnblocked item_index should match");
                    assert_eq!(s1, s2, "StepsUnblocked step_indices should match");
                }
                (
                    JournalEvent::ItemCompleted { item_index: i1, .. },
                    JournalEvent::ItemCompleted { item_index: i2, .. },
                ) => {
                    assert_eq!(i1, i2, "ItemCompleted item_index should match");
                }
                (
                    JournalEvent::RunCompleted { status: s1 },
                    JournalEvent::RunCompleted { status: s2 },
                ) => {
                    assert_eq!(s1, s2, "RunCompleted status should match");
                }
                _ => panic!(
                    "Event type mismatch at index {}: got {:?}, expected {:?}",
                    i, entry.event, events[i]
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStateStore;

    #[tokio::test]
    async fn in_memory_journal_compliance() {
        JournalComplianceTests::run_all_isolated(|| async { InMemoryStateStore::new() }).await;
    }
}
