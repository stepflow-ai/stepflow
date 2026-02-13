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

//! Tests for the SQL state store implementation.

use crate::SqliteStateStore;
use serde_json::json;
use std::collections::HashMap;
use stepflow_core::{BlobId, FlowResult, workflow::ValueRef};
use stepflow_state::{
    BlobStore as _, ExecutionJournal as _, JournalEntry, JournalEvent, SequenceNumber,
};
use uuid::Uuid;

#[tokio::test]
async fn test_blob_storage() {
    let store = SqliteStateStore::in_memory().await.unwrap();

    // Create test data
    let test_data = json!({"hello": "world", "number": 42});
    let value_ref = ValueRef::new(test_data.clone());

    // Store blob
    let blob_id = store
        .put_blob(
            value_ref.clone(),
            stepflow_core::BlobType::Data,
            Default::default(),
        )
        .await
        .unwrap();

    // Retrieve blob
    let retrieved = store.get_blob(&blob_id).await.unwrap();

    assert_eq!(retrieved.data().as_ref(), &test_data);
}

#[tokio::test]
async fn test_blob_deduplication() {
    let store = SqliteStateStore::in_memory().await.unwrap();

    let test_data = json!({"test": "data"});
    let value_ref = ValueRef::new(test_data);

    // Store the same data twice
    let blob_id1 = store
        .put_blob(
            value_ref.clone(),
            stepflow_core::BlobType::Data,
            Default::default(),
        )
        .await
        .unwrap();
    let blob_id2 = store
        .put_blob(value_ref, stepflow_core::BlobType::Data, Default::default())
        .await
        .unwrap();

    // Should get the same ID (deduplication)
    assert_eq!(blob_id1, blob_id2);
}

// =========================================================================
// ExecutionJournal Tests
// =========================================================================

#[tokio::test]
async fn test_journal_append_and_read() {
    let store = SqliteStateStore::in_memory().await.unwrap();
    let run_id = Uuid::now_v7();
    let root_run_id = run_id;

    // Append some journal entries
    // Use BlobId::from_content to generate a valid content-addressed ID
    let flow_id = BlobId::from_content(&ValueRef::new(json!({"test": "flow"}))).unwrap();
    let entry1 = JournalEntry::new(
        run_id,
        root_run_id,
        JournalEvent::RunCreated {
            flow_id,
            inputs: vec![ValueRef::new(json!({"key": "value"}))],
            variables: HashMap::new(),
            parent_run_id: None,
        },
    );

    let seq1 = store.append(entry1).await.unwrap();
    assert_eq!(seq1.value(), 0);

    let entry2 = JournalEntry::new(
        run_id,
        root_run_id,
        JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 0,
            result: FlowResult::Success(ValueRef::new(json!({}))),
        },
    );

    let seq2 = store.append(entry2).await.unwrap();
    assert_eq!(seq2.value(), 1);

    // Read entries from the beginning
    let entries = store
        .read_from(run_id, SequenceNumber::new(0), 10)
        .await
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0.value(), 0);
    assert_eq!(entries[1].0.value(), 1);

    // Read from a specific sequence
    let entries_from_1 = store
        .read_from(run_id, SequenceNumber::new(1), 10)
        .await
        .unwrap();
    assert_eq!(entries_from_1.len(), 1);
    assert_eq!(entries_from_1[0].0.value(), 1);
}

#[tokio::test]
async fn test_journal_latest_sequence() {
    let store = SqliteStateStore::in_memory().await.unwrap();
    let run_id = Uuid::now_v7();

    // Initially no entries
    let latest = store.latest_sequence(run_id).await.unwrap();
    assert!(latest.is_none());

    // Add an entry
    let entry = JournalEntry::new(
        run_id,
        run_id,
        JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 0,
            result: FlowResult::Success(ValueRef::new(json!({}))),
        },
    );
    store.append(entry).await.unwrap();

    // Now latest should be 0
    let latest = store.latest_sequence(run_id).await.unwrap();
    assert_eq!(latest, Some(SequenceNumber::new(0)));

    // Add another entry
    let entry2 = JournalEntry::new(
        run_id,
        run_id,
        JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 0,
            result: FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
        },
    );
    store.append(entry2).await.unwrap();

    // Latest should now be 1
    let latest = store.latest_sequence(run_id).await.unwrap();
    assert_eq!(latest, Some(SequenceNumber::new(1)));
}

#[tokio::test]
async fn test_journal_list_active_roots() {
    let store = SqliteStateStore::in_memory().await.unwrap();
    // Two separate root runs, each with its own journal
    let root_run_id1 = Uuid::now_v7();
    let root_run_id2 = Uuid::now_v7();

    // Add entries for two root runs (each is its own execution tree)
    for i in 0..3 {
        let entry = JournalEntry::new(
            root_run_id1,
            root_run_id1,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: i,
                result: FlowResult::Success(ValueRef::new(json!({}))),
            },
        );
        store.append(entry).await.unwrap();
    }

    for i in 0..2 {
        let entry = JournalEntry::new(
            root_run_id2,
            root_run_id2,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: i,
                result: FlowResult::Success(ValueRef::new(json!({}))),
            },
        );
        store.append(entry).await.unwrap();
    }

    // List active root runs
    let active_roots = store.list_active_roots().await.unwrap();
    assert_eq!(active_roots.len(), 2);

    // Find root1 info
    let root1_info = active_roots
        .iter()
        .find(|r| r.root_run_id == root_run_id1)
        .unwrap();
    assert_eq!(root1_info.latest_sequence, SequenceNumber::new(2));
    assert_eq!(root1_info.entry_count, 3);

    // Find root2 info
    let root2_info = active_roots
        .iter()
        .find(|r| r.root_run_id == root_run_id2)
        .unwrap();
    assert_eq!(root2_info.latest_sequence, SequenceNumber::new(1));
    assert_eq!(root2_info.entry_count, 2);
}

#[tokio::test]
async fn test_journal_read_with_limit() {
    let store = SqliteStateStore::in_memory().await.unwrap();
    let run_id = Uuid::now_v7();

    // Add 10 entries
    for i in 0..10 {
        let entry = JournalEntry::new(
            run_id,
            run_id,
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: i,
                result: FlowResult::Success(ValueRef::new(json!({}))),
            },
        );
        store.append(entry).await.unwrap();
    }

    // Read with limit of 3
    let entries = store
        .read_from(run_id, SequenceNumber::new(0), 3)
        .await
        .unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].0.value(), 0);
    assert_eq!(entries[2].0.value(), 2);

    // Read with limit starting from sequence 5
    let entries = store
        .read_from(run_id, SequenceNumber::new(5), 3)
        .await
        .unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].0.value(), 5);
    assert_eq!(entries[2].0.value(), 7);
}

#[tokio::test]
async fn test_journal_subflow_shared_journal() {
    // Test that parent and subflow events share the same journal keyed by root_run_id
    let store = SqliteStateStore::in_memory().await.unwrap();

    // Create IDs for parent and subflow
    let root_run_id = Uuid::now_v7();
    let parent_run_id = root_run_id; // Parent is the root
    let subflow_run_id = Uuid::now_v7();

    // Write interleaved events from parent and subflow
    // This simulates real execution where parent spawns subflow mid-execution

    // Parent: TaskCompleted for step 0
    let entry1 = JournalEntry::new(
        parent_run_id,
        root_run_id,
        JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 0,
            result: FlowResult::Success(ValueRef::new(json!({"parent": 0}))),
        },
    );
    let seq1 = store.append(entry1).await.unwrap();
    assert_eq!(seq1.value(), 0);

    // Subflow: TaskCompleted for step 0 (subflow begins)
    let entry2 = JournalEntry::new(
        subflow_run_id,
        root_run_id,
        JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 0,
            result: FlowResult::Success(ValueRef::new(json!({"subflow": 0}))),
        },
    );
    let seq2 = store.append(entry2).await.unwrap();
    assert_eq!(seq2.value(), 1);

    // Subflow: TaskCompleted for step 1
    let entry3 = JournalEntry::new(
        subflow_run_id,
        root_run_id,
        JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 1,
            result: FlowResult::Success(ValueRef::new(json!({"subflow": 1}))),
        },
    );
    let seq3 = store.append(entry3).await.unwrap();
    assert_eq!(seq3.value(), 2);

    // Parent: TaskCompleted for step 1 (after subflow completes)
    let entry4 = JournalEntry::new(
        parent_run_id,
        root_run_id,
        JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 1,
            result: FlowResult::Success(ValueRef::new(json!({"parent": 1}))),
        },
    );
    let seq4 = store.append(entry4).await.unwrap();
    assert_eq!(seq4.value(), 3);

    // Read all entries - should get all 4 in sequence order
    let all_entries = store
        .read_from(root_run_id, SequenceNumber::new(0), usize::MAX)
        .await
        .unwrap();
    assert_eq!(all_entries.len(), 4);

    // Verify sequence numbers are monotonic
    for (i, (seq, _)) in all_entries.iter().enumerate() {
        assert_eq!(seq.value(), i as u64);
    }

    // Filter for parent events only
    let parent_entries: Vec<_> = all_entries
        .iter()
        .filter(|(_, e)| e.run_id == parent_run_id)
        .collect();
    assert_eq!(parent_entries.len(), 2);
    assert_eq!(parent_entries[0].0.value(), 0); // First parent event
    assert_eq!(parent_entries[1].0.value(), 3); // Second parent event

    // Filter for subflow events only
    let subflow_entries: Vec<_> = all_entries
        .iter()
        .filter(|(_, e)| e.run_id == subflow_run_id)
        .collect();
    assert_eq!(subflow_entries.len(), 2);
    assert_eq!(subflow_entries[0].0.value(), 1); // First subflow event
    assert_eq!(subflow_entries[1].0.value(), 2); // Second subflow event

    // Verify list_active_roots only shows one root journal
    let active_roots = store.list_active_roots().await.unwrap();
    assert_eq!(active_roots.len(), 1);
    assert_eq!(active_roots[0].root_run_id, root_run_id);
    assert_eq!(active_roots[0].latest_sequence, SequenceNumber::new(3));
}

// =========================================================================
// ExecutionJournal Compliance Tests
// =========================================================================

#[tokio::test]
async fn sqlite_journal_compliance() {
    use stepflow_state::journal_compliance::JournalComplianceTests;

    JournalComplianceTests::run_all_isolated(|| async {
        SqliteStateStore::in_memory().await.unwrap()
    })
    .await;
}

// =========================================================================
// MetadataStore Compliance Tests
// =========================================================================

#[tokio::test]
async fn sqlite_metadata_compliance() {
    use stepflow_state::metadata_compliance::MetadataComplianceTests;

    MetadataComplianceTests::run_all_isolated(|| async {
        SqliteStateStore::in_memory().await.unwrap()
    })
    .await;
}

// =========================================================================
// BlobStore Compliance Tests
// =========================================================================

#[tokio::test]
async fn sqlite_blob_compliance() {
    use stepflow_state::blob_compliance::BlobStoreComplianceTests;

    BlobStoreComplianceTests::run_all_isolated(|| async {
        SqliteStateStore::in_memory().await.unwrap()
    })
    .await;
}
