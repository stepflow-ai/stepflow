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

use crate::{
    ExecutionJournal, ItemSteps, JournalEvent, RunTaskAttempts, SequenceNumber, TaskAttempt,
};

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
            let seq = journal
                .write(
                    root_run_id,
                    JournalEvent::TaskCompleted {
                        run_id,
                        item_index: 0,
                        step_index: i,
                        result: FlowResult::Success(ValueRef::new(json!({}))),
                    },
                )
                .await
                .expect("write should succeed");

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
        let events = journal
            .read_from(root_run_id, SequenceNumber::new(0), 100)
            .await
            .expect("read_from should succeed");
        assert!(
            events.is_empty(),
            "read_from non-existent journal should return empty vec"
        );
    }

    /// Test that read_from() returns events in sequence order.
    ///
    /// Contract: Events are returned in ascending sequence number order.
    pub async fn test_read_from_returns_entries_in_order<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let run_id = root_run_id;

        // Append events and track returned sequence numbers
        let mut appended_seqs = Vec::new();
        for i in 0..5 {
            let seq = journal
                .write(
                    root_run_id,
                    JournalEvent::TaskCompleted {
                        run_id,
                        item_index: 0,
                        step_index: i,
                        result: FlowResult::Success(ValueRef::new(json!({}))),
                    },
                )
                .await
                .expect("write should succeed");
            appended_seqs.push(seq);
        }

        // Read all events starting from the first appended sequence
        let events = journal
            .read_from(root_run_id, appended_seqs[0], 100)
            .await
            .expect("read_from should succeed");

        assert_eq!(events.len(), 5, "Should have 5 events");

        // Verify events are in correct order by checking step_index
        for (i, event) in events.iter().enumerate() {
            match event {
                JournalEvent::TaskCompleted { step_index, .. } => {
                    assert_eq!(*step_index, i, "Event {i} should have step_index {i}");
                }
                _ => panic!("Expected TaskCompleted event"),
            }
        }
    }

    /// Test that read_from() respects the from_sequence parameter.
    ///
    /// Contract: Only events with sequence >= from_sequence are returned.
    pub async fn test_read_from_respects_from_sequence<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let run_id = root_run_id;

        // Append 10 events and track sequence numbers
        let mut appended_seqs = Vec::new();
        for i in 0..10 {
            let seq = journal
                .write(
                    root_run_id,
                    JournalEvent::TaskCompleted {
                        run_id,
                        item_index: 0,
                        step_index: i,
                        result: FlowResult::Success(ValueRef::new(json!({}))),
                    },
                )
                .await
                .expect("write should succeed");
            appended_seqs.push(seq);
        }

        // Read from the 6th sequence (index 5)
        let events = journal
            .read_from(root_run_id, appended_seqs[5], 100)
            .await
            .expect("read_from should succeed");

        assert_eq!(events.len(), 5, "Should have 5 events (indices 5-9)");
        // Verify the first event has step_index 5
        match &events[0] {
            JournalEvent::TaskCompleted { step_index, .. } => {
                assert_eq!(*step_index, 5, "First event should have step_index 5");
            }
            _ => panic!("Expected TaskCompleted event"),
        }
    }

    /// Test that read_from() respects the limit parameter.
    ///
    /// Contract: At most `limit` events are returned.
    pub async fn test_read_from_respects_limit<J: ExecutionJournal>(journal: &J) {
        let root_run_id = Uuid::now_v7();
        let run_id = root_run_id;

        // Append 10 events
        let mut appended_seqs = Vec::new();
        for i in 0..10 {
            let seq = journal
                .write(
                    root_run_id,
                    JournalEvent::TaskCompleted {
                        run_id,
                        item_index: 0,
                        step_index: i,
                        result: FlowResult::Success(ValueRef::new(json!({}))),
                    },
                )
                .await
                .expect("write should succeed");
            appended_seqs.push(seq);
        }

        // Read with limit of 3
        let events = journal
            .read_from(root_run_id, appended_seqs[0], 3)
            .await
            .expect("read_from should succeed");

        assert_eq!(events.len(), 3, "Should have exactly 3 events");

        // Read with limit of 0
        let events = journal
            .read_from(root_run_id, appended_seqs[0], 0)
            .await
            .expect("read_from should succeed");
        assert!(events.is_empty(), "Limit 0 should return empty vec");
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

        // Append first event
        let seq1 = journal
            .write(
                root_run_id,
                JournalEvent::TaskCompleted {
                    run_id,
                    item_index: 0,
                    step_index: 0,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            )
            .await
            .expect("write should succeed");

        let latest = journal
            .latest_sequence(root_run_id)
            .await
            .expect("latest_sequence should succeed");
        assert_eq!(
            latest,
            Some(seq1),
            "latest_sequence should match first append"
        );

        // Append more events, tracking the last one
        let mut last_seq = seq1;
        for i in 1..5 {
            last_seq = journal
                .write(
                    root_run_id,
                    JournalEvent::TaskCompleted {
                        run_id,
                        item_index: 0,
                        step_index: i,
                        result: FlowResult::Success(ValueRef::new(json!({}))),
                    },
                )
                .await
                .expect("write should succeed");
        }

        let latest = journal
            .latest_sequence(root_run_id)
            .await
            .expect("latest_sequence should succeed");
        assert_eq!(
            latest,
            Some(last_seq),
            "latest_sequence should match last appended sequence"
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

        // Add entries to each, tracking the last sequence for each root
        let mut root1_last_seq = None;
        for i in 0..3 {
            root1_last_seq = Some(
                journal
                    .write(
                        root1,
                        JournalEvent::TaskCompleted {
                            run_id: root1,
                            item_index: 0,
                            step_index: i,
                            result: FlowResult::Success(ValueRef::new(json!({}))),
                        },
                    )
                    .await
                    .expect("write should succeed"),
            );
        }

        let mut root2_last_seq = None;
        for i in 0..5 {
            root2_last_seq = Some(
                journal
                    .write(
                        root2,
                        JournalEvent::TaskCompleted {
                            run_id: root2,
                            item_index: 0,
                            step_index: i,
                            result: FlowResult::Success(ValueRef::new(json!({}))),
                        },
                    )
                    .await
                    .expect("write should succeed"),
            );
        }

        let root3_last_seq = journal
            .write(
                root3,
                JournalEvent::TaskCompleted {
                    run_id: root3,
                    item_index: 0,
                    step_index: 0,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            )
            .await
            .expect("write should succeed");

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
        assert_eq!(
            root1_info.latest_sequence,
            root1_last_seq.unwrap(),
            "root1 latest_sequence should match last appended"
        );
        assert_eq!(root1_info.entry_count, 3);

        let root2_info = root2_info.unwrap();
        assert_eq!(
            root2_info.latest_sequence,
            root2_last_seq.unwrap(),
            "root2 latest_sequence should match last appended"
        );
        assert_eq!(root2_info.entry_count, 5);

        let root3_info = root3_info.unwrap();
        assert_eq!(
            root3_info.latest_sequence, root3_last_seq,
            "root3 latest_sequence should match last appended"
        );
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
        let _seq1 = journal
            .write(
                root_run_id,
                JournalEvent::TaskCompleted {
                    run_id: parent_run_id,
                    item_index: 0,
                    step_index: 0,
                    result: FlowResult::Success(ValueRef::new(json!({"parent": 1}))),
                },
            )
            .await
            .expect("write should succeed");

        let _seq2 = journal
            .write(
                root_run_id,
                JournalEvent::TaskCompleted {
                    run_id: subflow_run_id,
                    item_index: 0,
                    step_index: 0,
                    result: FlowResult::Success(ValueRef::new(json!({"subflow": 1}))),
                },
            )
            .await
            .expect("write should succeed");

        let _seq3 = journal
            .write(
                root_run_id,
                JournalEvent::TaskCompleted {
                    run_id: subflow_run_id,
                    item_index: 0,
                    step_index: 0,
                    result: FlowResult::Success(ValueRef::new(json!({"done": true}))),
                },
            )
            .await
            .expect("write should succeed");

        let _seq4 = journal
            .write(
                root_run_id,
                JournalEvent::TaskCompleted {
                    run_id: parent_run_id,
                    item_index: 0,
                    step_index: 0,
                    result: FlowResult::Success(ValueRef::new(json!({"done": true}))),
                },
            )
            .await
            .expect("write should succeed");

        // Read all events from the shared journal
        let all_events = journal
            .read_from(root_run_id, SequenceNumber::new(0), 100)
            .await
            .expect("read_from should succeed");
        assert_eq!(
            all_events.len(),
            4,
            "Should have 4 events in shared journal"
        );

        // Verify events have expected run_ids (2 parent, 2 subflow)
        let parent_count = all_events
            .iter()
            .filter(|e| matches!(e, JournalEvent::TaskCompleted { run_id, .. } if *run_id == parent_run_id))
            .count();
        assert_eq!(parent_count, 2, "Should have 2 parent events");

        let subflow_count = all_events
            .iter()
            .filter(|e| matches!(e, JournalEvent::TaskCompleted { run_id, .. } if *run_id == subflow_run_id))
            .count();
        assert_eq!(subflow_count, 2, "Should have 2 subflow events");

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
            JournalEvent::RootRunCreated {
                run_id,
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
            },
            JournalEvent::SubRunCreated {
                run_id: Uuid::now_v7(),
                flow_id: flow_id.clone(),
                inputs: vec![ValueRef::new(json!({"sub": true}))],
                variables: HashMap::new(),
                parent_run_id: run_id,
                item_index: 0,
                step_index: 0,
                subflow_key: Uuid::now_v7(),
            },
            JournalEvent::RunInitialized {
                run_id,
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
            JournalEvent::TasksStarted {
                runs: vec![RunTaskAttempts {
                    run_id,
                    tasks: vec![TaskAttempt::new(0, 0, 1), TaskAttempt::new(0, 1, 1)],
                }],
            },
            JournalEvent::TaskCompleted {
                run_id,
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            },
            JournalEvent::TaskCompleted {
                run_id,
                item_index: 0,
                step_index: 1,
                result: FlowResult::Failed(FlowError {
                    code: 1,
                    message: "test error".into(),
                    data: None,
                }),
            },
            JournalEvent::StepsUnblocked {
                run_id,
                item_index: 0,
                step_indices: vec![2, 3, 4],
            },
            JournalEvent::ItemCompleted {
                run_id,
                item_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"item": "done"}))),
            },
            JournalEvent::RunCompleted {
                run_id,
                status: stepflow_core::status::ExecutionStatus::Completed,
            },
        ];

        // Append all events
        for event in &events {
            journal
                .write(root_run_id, event.clone())
                .await
                .expect("write should succeed");
        }

        // Read all events back
        let read_events = journal
            .read_from(root_run_id, SequenceNumber::new(0), 100)
            .await
            .expect("read_from should succeed");

        assert_eq!(read_events.len(), events.len(), "Should have all events");

        // Verify each event type was preserved
        for (i, event) in read_events.iter().enumerate() {
            match (event, &events[i]) {
                (
                    JournalEvent::RootRunCreated { flow_id: f1, .. },
                    JournalEvent::RootRunCreated { flow_id: f2, .. },
                ) => {
                    assert_eq!(f1, f2, "RootRunCreated flow_id should match");
                }
                (
                    JournalEvent::SubRunCreated { flow_id: f1, .. },
                    JournalEvent::SubRunCreated { flow_id: f2, .. },
                ) => {
                    assert_eq!(f1, f2, "SubRunCreated flow_id should match");
                }
                (
                    JournalEvent::RunInitialized {
                        needed_steps: n1, ..
                    },
                    JournalEvent::RunInitialized {
                        needed_steps: n2, ..
                    },
                ) => {
                    assert_eq!(
                        n1.len(),
                        n2.len(),
                        "RunInitialized needed_steps length should match"
                    );
                }
                (
                    JournalEvent::TasksStarted { runs: r1 },
                    JournalEvent::TasksStarted { runs: r2 },
                ) => {
                    assert_eq!(r1.len(), r2.len(), "TasksStarted runs length should match");
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
                        ..
                    },
                    JournalEvent::StepsUnblocked {
                        item_index: i2,
                        step_indices: s2,
                        ..
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
                    JournalEvent::RunCompleted { status: s1, .. },
                    JournalEvent::RunCompleted { status: s2, .. },
                ) => {
                    assert_eq!(s1, s2, "RunCompleted status should match");
                }
                _ => panic!(
                    "Event type mismatch at index {}: got {:?}, expected {:?}",
                    i, event, events[i]
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
