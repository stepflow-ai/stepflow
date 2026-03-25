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
use futures::StreamExt as _;
use serde_json::json;
use std::collections::HashMap;
use stepflow_core::{BlobId, FlowResult, workflow::ValueRef};
use stepflow_state::{BlobStore as _, ExecutionJournal as _, JournalEvent, SequenceNumber};
use uuid::Uuid;

#[tokio::test]
async fn test_blob_storage() {
    let store = SqliteStateStore::in_memory().await.unwrap();

    // Create test data
    let test_data = json!({"hello": "world", "number": 42});
    let content = serde_json::to_vec(&test_data).unwrap();

    // Store blob
    let blob_id = store
        .put_blob(&content, stepflow_core::BlobType::Data, Default::default())
        .await
        .unwrap();

    // Retrieve blob
    let raw = store
        .get_blob(&blob_id)
        .await
        .unwrap()
        .expect("Blob should exist");

    let retrieved: serde_json::Value = serde_json::from_slice(&raw.content).unwrap();
    assert_eq!(retrieved, test_data);
}

#[tokio::test]
async fn test_blob_deduplication() {
    let store = SqliteStateStore::in_memory().await.unwrap();

    let test_data = json!({"test": "data"});
    let content = serde_json::to_vec(&test_data).unwrap();

    // Store the same data twice
    let blob_id1 = store
        .put_blob(&content, stepflow_core::BlobType::Data, Default::default())
        .await
        .unwrap();
    let blob_id2 = store
        .put_blob(&content, stepflow_core::BlobType::Data, Default::default())
        .await
        .unwrap();

    // Should get the same ID (deduplication)
    assert_eq!(blob_id1, blob_id2);
}

// =========================================================================
// ExecutionJournal Tests
// =========================================================================

#[tokio::test]
async fn test_journal_write_and_read() {
    let store = SqliteStateStore::in_memory().await.unwrap();
    let run_id = Uuid::now_v7();
    let root_run_id = run_id;

    // Append some journal entries
    // Use BlobId::from_content to generate a valid content-addressed ID
    let flow_id = BlobId::from_content(&ValueRef::new(json!({"test": "flow"}))).unwrap();
    let seq1 = store
        .write(
            root_run_id,
            JournalEvent::RootRunCreated {
                run_id,
                flow_id,
                inputs: vec![ValueRef::new(json!({"key": "value"}))],
                variables: HashMap::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(seq1.value(), 0);

    let seq2 = store
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
        .unwrap();
    assert_eq!(seq2.value(), 1);

    // Stream entries from the beginning, checking each element
    let mut stream = store.stream_from(run_id, SequenceNumber::new(0));
    let entry = stream.next().await.unwrap().unwrap();
    assert!(matches!(entry.event, JournalEvent::RootRunCreated { .. }));
    assert_eq!(entry.sequence, seq1);
    let entry = stream.next().await.unwrap().unwrap();
    assert!(matches!(entry.event, JournalEvent::TaskCompleted { .. }));
    assert_eq!(entry.sequence, seq2);
    assert!(stream.next().await.is_none());

    // Stream from a specific sequence
    let mut stream = store.stream_from(run_id, SequenceNumber::new(1));
    let entry = stream.next().await.unwrap().unwrap();
    assert!(matches!(entry.event, JournalEvent::TaskCompleted { .. }));
    assert_eq!(entry.sequence, seq2);
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn test_journal_latest_sequence() {
    let store = SqliteStateStore::in_memory().await.unwrap();
    let run_id = Uuid::now_v7();

    // Initially no entries
    let latest = store.latest_sequence(run_id).await.unwrap();
    assert!(latest.is_none());

    // Add an entry
    store
        .write(
            run_id,
            JournalEvent::TaskCompleted {
                run_id,
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({}))),
            },
        )
        .await
        .unwrap();

    // Now latest should be 0
    let latest = store.latest_sequence(run_id).await.unwrap();
    assert_eq!(latest, Some(SequenceNumber::new(0)));

    // Add another entry
    store
        .write(
            run_id,
            JournalEvent::TaskCompleted {
                run_id,
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            },
        )
        .await
        .unwrap();

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
        store
            .write(
                root_run_id1,
                JournalEvent::TaskCompleted {
                    run_id: root_run_id1,
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            )
            .await
            .unwrap();
    }

    for i in 0..2 {
        store
            .write(
                root_run_id2,
                JournalEvent::TaskCompleted {
                    run_id: root_run_id2,
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            )
            .await
            .unwrap();
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
async fn test_journal_stream_from_sequence() {
    let store = SqliteStateStore::in_memory().await.unwrap();
    let run_id = Uuid::now_v7();

    // Add 10 entries
    for i in 0..10 {
        store
            .write(
                run_id,
                JournalEvent::TaskCompleted {
                    run_id,
                    item_index: 0,
                    step_index: i,
                    result: FlowResult::Success(ValueRef::new(json!({}))),
                },
            )
            .await
            .unwrap();
    }

    // Stream all entries, verify count and first/last
    let mut stream = store.stream_from(run_id, SequenceNumber::new(0));
    for expected_step in 0..10 {
        let entry = stream.next().await.unwrap().unwrap();
        let event = entry.event;
        match event {
            JournalEvent::TaskCompleted { step_index, .. } => {
                assert_eq!(step_index, expected_step);
            }
            _ => panic!("Expected TaskCompleted"),
        }
    }
    assert!(stream.next().await.is_none());

    // Stream starting from sequence 5
    let mut stream = store.stream_from(run_id, SequenceNumber::new(5));
    for expected_step in 5..10 {
        let entry = stream.next().await.unwrap().unwrap();
        let event = entry.event;
        match event {
            JournalEvent::TaskCompleted { step_index, .. } => {
                assert_eq!(step_index, expected_step);
            }
            _ => panic!("Expected TaskCompleted"),
        }
    }
    assert!(stream.next().await.is_none());
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
    let seq1 = store
        .write(
            root_run_id,
            JournalEvent::TaskCompleted {
                run_id: parent_run_id,
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"parent": 0}))),
            },
        )
        .await
        .unwrap();
    assert_eq!(seq1.value(), 0);

    // Subflow: TaskCompleted for step 0 (subflow begins)
    let seq2 = store
        .write(
            root_run_id,
            JournalEvent::TaskCompleted {
                run_id: subflow_run_id,
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"subflow": 0}))),
            },
        )
        .await
        .unwrap();
    assert_eq!(seq2.value(), 1);

    // Subflow: TaskCompleted for step 1
    let seq3 = store
        .write(
            root_run_id,
            JournalEvent::TaskCompleted {
                run_id: subflow_run_id,
                item_index: 0,
                step_index: 1,
                result: FlowResult::Success(ValueRef::new(json!({"subflow": 1}))),
            },
        )
        .await
        .unwrap();
    assert_eq!(seq3.value(), 2);

    // Parent: TaskCompleted for step 1 (after subflow completes)
    let seq4 = store
        .write(
            root_run_id,
            JournalEvent::TaskCompleted {
                run_id: parent_run_id,
                item_index: 0,
                step_index: 1,
                result: FlowResult::Success(ValueRef::new(json!({"parent": 1}))),
            },
        )
        .await
        .unwrap();
    assert_eq!(seq4.value(), 3);

    // Stream all entries and verify the interleaved order
    let mut stream = store.stream_from(root_run_id, SequenceNumber::new(0));

    // Event 1: parent step 0
    let entry = stream.next().await.unwrap().unwrap();
    let event = entry.event;
    assert!(
        matches!(event, JournalEvent::TaskCompleted { run_id, step_index: 0, .. } if run_id == parent_run_id)
    );

    // Event 2: subflow step 0
    let entry = stream.next().await.unwrap().unwrap();
    let event = entry.event;
    assert!(
        matches!(event, JournalEvent::TaskCompleted { run_id, step_index: 0, .. } if run_id == subflow_run_id)
    );

    // Event 3: subflow step 1
    let entry = stream.next().await.unwrap().unwrap();
    let event = entry.event;
    assert!(
        matches!(event, JournalEvent::TaskCompleted { run_id, step_index: 1, .. } if run_id == subflow_run_id)
    );

    // Event 4: parent step 1
    let entry = stream.next().await.unwrap().unwrap();
    let event = entry.event;
    assert!(
        matches!(event, JournalEvent::TaskCompleted { run_id, step_index: 1, .. } if run_id == parent_run_id)
    );

    assert!(stream.next().await.is_none());

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

// =========================================================================
// Migration Idempotency Tests
// =========================================================================

/// Migrations must be idempotent: calling run_all_migrations twice on the same
/// database must succeed. This exercises the `INSERT OR IGNORE` and
/// concurrent-failure-recheck logic in `apply_migration`.
#[tokio::test]
async fn test_migrations_idempotent() {
    // First call creates all tables and records migrations
    let store = SqliteStateStore::in_memory().await.unwrap();

    // Second call on the same pool should be a no-op (all migrations already applied)
    crate::migrations::run_all_migrations(&store.pool)
        .await
        .unwrap();
}

/// Two separate pools connected to the same file-based SQLite database must
/// both be able to run migrations without conflict. This simulates two
/// orchestrators starting concurrently against a shared database.
#[tokio::test]
async fn test_migrations_concurrent_pools() {
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::NamedTempFile;

    let tmp = NamedTempFile::new().unwrap();
    let url = format!("sqlite:{}?mode=rwc", tmp.path().display());

    let pool1 = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let pool2 = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();

    // Enable WAL mode on both (mirrors production setup)
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool1)
        .await
        .unwrap();
    sqlx::query("PRAGMA busy_timeout=5000")
        .execute(&pool1)
        .await
        .unwrap();
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool2)
        .await
        .unwrap();
    sqlx::query("PRAGMA busy_timeout=5000")
        .execute(&pool2)
        .await
        .unwrap();

    // Run migrations from both pools concurrently
    let (r1, r2) = tokio::join!(
        crate::migrations::run_all_migrations(&pool1),
        crate::migrations::run_all_migrations(&pool2),
    );

    r1.expect("pool1 migrations should succeed");
    r2.expect("pool2 migrations should succeed");

    // Verify both can read/write after migrations
    let store1 = SqliteStateStore::new(crate::SqliteStateStoreConfig {
        database_url: url.clone(),
        max_connections: 1,
        auto_migrate: false, // Already migrated
    })
    .await
    .unwrap();

    let test_data = serde_json::json!({"test": "concurrent"});
    let blob_bytes = serde_json::to_vec(&test_data).unwrap();
    store1
        .put_blob(
            &blob_bytes,
            stepflow_core::BlobType::Data,
            Default::default(),
        )
        .await
        .unwrap();
}

// =========================================================================
// Selective Migration Tests
// =========================================================================

/// Helper to check if a table exists in the database.
async fn table_exists(pool: &sqlx::SqlitePool, table_name: &str) -> bool {
    let row =
        sqlx::query("SELECT COUNT(*) as count FROM sqlite_master WHERE type='table' AND name=?")
            .bind(table_name)
            .fetch_one(pool)
            .await
            .unwrap();
    let count: i64 = sqlx::Row::get(&row, "count");
    count > 0
}

/// Blob-only migration should only create the blobs table.
#[tokio::test]
async fn test_blob_only_migration() {
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    crate::migrations::run_blob_migrations(&pool).await.unwrap();

    assert!(table_exists(&pool, "blobs").await);
    assert!(!table_exists(&pool, "runs").await);
    assert!(!table_exists(&pool, "step_results").await);
    assert!(!table_exists(&pool, "step_info").await);
    assert!(!table_exists(&pool, "run_items").await);
    assert!(!table_exists(&pool, "journal_entries").await);
}

/// Metadata migration should create only metadata tables (no blobs, no journal).
/// Blobs may live in a different backend (e.g., S3), so no FK or table dependency.
#[tokio::test]
async fn test_metadata_only_migration() {
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    crate::migrations::run_metadata_migrations(&pool)
        .await
        .unwrap();

    // Blob tables NOT created (blobs are independent)
    assert!(!table_exists(&pool, "blobs").await);
    // Metadata tables created
    assert!(table_exists(&pool, "runs").await);
    assert!(table_exists(&pool, "step_results").await);
    assert!(table_exists(&pool, "step_info").await);
    assert!(table_exists(&pool, "run_items").await);
    // Journal not created
    assert!(!table_exists(&pool, "journal_entries").await);
}

/// Journal-only migration should only create the journal_entries table.
#[tokio::test]
async fn test_journal_only_migration() {
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    crate::migrations::run_journal_migrations(&pool)
        .await
        .unwrap();

    assert!(!table_exists(&pool, "blobs").await);
    assert!(!table_exists(&pool, "runs").await);
    assert!(table_exists(&pool, "journal_entries").await);
}

// =========================================================================
// CheckpointStore Compliance Tests
// =========================================================================

#[tokio::test]
async fn sqlite_checkpoint_compliance() {
    use stepflow_state::checkpoint_compliance::CheckpointComplianceTests;

    CheckpointComplianceTests::run_all_isolated(|| async {
        SqliteStateStore::in_memory().await.unwrap()
    })
    .await;
}

/// Checkpoint migration should only create the checkpoints table.
#[tokio::test]
async fn test_checkpoint_only_migration() {
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    crate::migrations::run_checkpoint_migrations(&pool)
        .await
        .unwrap();

    assert!(table_exists(&pool, "checkpoints").await);
    assert!(!table_exists(&pool, "blobs").await);
    assert!(!table_exists(&pool, "runs").await);
    assert!(!table_exists(&pool, "journal_entries").await);
}

/// Running migrations incrementally should add tables for new store types.
#[tokio::test]
async fn test_incremental_migrations() {
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    // First: only blob tables
    crate::migrations::run_blob_migrations(&pool).await.unwrap();

    assert!(table_exists(&pool, "blobs").await);
    assert!(!table_exists(&pool, "runs").await);

    // Second: add metadata tables
    crate::migrations::run_metadata_migrations(&pool)
        .await
        .unwrap();

    assert!(table_exists(&pool, "blobs").await);
    assert!(table_exists(&pool, "runs").await);
    assert!(table_exists(&pool, "step_results").await);
    assert!(table_exists(&pool, "run_items").await);
}

// =========================================================================
// Step status round-trip tests (issue #849)
// =========================================================================

/// Helper: create a store with a run so we can write step statuses.
async fn create_store_with_run() -> (SqliteStateStore, Uuid) {
    use std::sync::Arc;
    use stepflow_core::ValueExpr;
    use stepflow_core::workflow::{FlowBuilder, StepBuilder};
    use stepflow_state::{BlobStore as _, CreateRunParams, MetadataStore as _};

    let store = SqliteStateStore::in_memory().await.unwrap();

    let flow = Arc::new(
        FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::new("classify")
                    .component("/mock/classify")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::new("convert")
                    .component("/mock/convert")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::new("chunk")
                    .component("/mock/chunk")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "chunk".to_string(),
                path: Default::default(),
            })
            .build(),
    );

    let flow_id = store.store_flow(flow).await.unwrap();
    let run_id = Uuid::now_v7();
    let input = stepflow_core::workflow::ValueRef::new(json!({"file": "test.pdf"}));

    store
        .create_run(CreateRunParams::new(
            run_id,
            flow_id,
            vec![input],
            SequenceNumber::new(0),
        ))
        .await
        .unwrap();

    (store, run_id)
}

#[tokio::test]
async fn test_step_status_round_trip() {
    use stepflow_core::status::StepStatus;
    use stepflow_state::MetadataStore as _;

    let (store, run_id) = create_store_with_run().await;

    // Write step statuses for 3 steps
    let small_result = FlowResult::from(json!({"doc_type": "scientific_paper"}));
    let medium_result = FlowResult::from(json!({"chunks": ["chunk1", "chunk2", "chunk3"]}));

    store
        .update_step_status(
            run_id,
            0,
            "classify",
            0,
            StepStatus::Completed,
            Some("/mock/classify"),
            Some(small_result.clone()),
            SequenceNumber::new(1),
        )
        .await
        .unwrap();

    store
        .update_step_status(
            run_id,
            0,
            "convert",
            1,
            StepStatus::Completed,
            Some("/mock/convert"),
            Some(medium_result.clone()),
            SequenceNumber::new(2),
        )
        .await
        .unwrap();

    store
        .update_step_status(
            run_id,
            0,
            "chunk",
            2,
            StepStatus::Completed,
            Some("/mock/chunk"),
            Some(medium_result.clone()),
            SequenceNumber::new(3),
        )
        .await
        .unwrap();

    // Read them back
    let statuses = store
        .get_step_statuses(run_id, None, None)
        .await
        .expect("get_step_statuses should succeed");

    assert_eq!(statuses.len(), 3, "Should have 3 step statuses");
    assert_eq!(statuses[0].step_id, "classify");
    assert_eq!(statuses[0].status, StepStatus::Completed);
    assert_eq!(statuses[0].result, Some(small_result));
    assert_eq!(statuses[1].step_id, "convert");
    assert_eq!(statuses[2].step_id, "chunk");
}

#[tokio::test]
async fn test_step_status_large_result_json() {
    use stepflow_core::status::StepStatus;
    use stepflow_state::MetadataStore as _;

    let (store, run_id) = create_store_with_run().await;

    // Build a ~2MB JSON payload similar to what the convert step produces
    let large_text = "x".repeat(2_000_000);
    let large_result = FlowResult::from(json!({
        "pages": [
            {"page_num": 1, "text": large_text},
        ],
        "metadata": {"format": "pdf", "page_count": 1}
    }));

    store
        .update_step_status(
            run_id,
            0,
            "convert",
            1,
            StepStatus::Completed,
            Some("/mock/convert"),
            Some(large_result.clone()),
            SequenceNumber::new(1),
        )
        .await
        .expect("writing large result_json should succeed");

    // Read it back — this is the exact code path that fails in issue #849
    let statuses = store
        .get_step_statuses(run_id, None, None)
        .await
        .expect("get_step_statuses with large result_json should succeed");

    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].step_id, "convert");
    assert_eq!(statuses[0].result, Some(large_result));
}

#[tokio::test]
async fn test_step_status_with_none_result() {
    use stepflow_core::status::StepStatus;
    use stepflow_state::MetadataStore as _;

    let (store, run_id) = create_store_with_run().await;

    // Write a step status without a result (e.g., step is Running)
    store
        .update_step_status(
            run_id,
            0,
            "classify",
            0,
            StepStatus::Running,
            Some("/mock/classify"),
            None,
            SequenceNumber::new(1),
        )
        .await
        .unwrap();

    let statuses = store
        .get_step_statuses(run_id, None, None)
        .await
        .expect("get_step_statuses with null result should succeed");

    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].status, StepStatus::Running);
    assert!(statuses[0].result.is_none());
}

#[tokio::test]
async fn test_step_status_update_then_read() {
    use stepflow_core::status::StepStatus;
    use stepflow_state::MetadataStore as _;

    let (store, run_id) = create_store_with_run().await;

    // First write: Running, no result
    store
        .update_step_status(
            run_id,
            0,
            "classify",
            0,
            StepStatus::Running,
            Some("/mock/classify"),
            None,
            SequenceNumber::new(1),
        )
        .await
        .unwrap();

    // Second write: Completed, with result (triggers ON CONFLICT UPDATE)
    let result = FlowResult::from(json!({"doc_type": "invoice"}));
    store
        .update_step_status(
            run_id,
            0,
            "classify",
            0,
            StepStatus::Completed,
            Some("/mock/classify"),
            Some(result.clone()),
            SequenceNumber::new(2),
        )
        .await
        .unwrap();

    let statuses = store
        .get_step_statuses(run_id, None, None)
        .await
        .expect("get_step_statuses after update should succeed");

    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].status, StepStatus::Completed);
    assert_eq!(statuses[0].result, Some(result));
    assert_eq!(statuses[0].journal_seqno, 2);
}

#[tokio::test]
async fn test_step_status_failed_result_round_trip() {
    use stepflow_core::TaskErrorCode;
    use stepflow_core::status::StepStatus;
    use stepflow_state::MetadataStore as _;

    let (store, run_id) = create_store_with_run().await;

    let error = stepflow_core::FlowError::new(TaskErrorCode::ComponentFailed, "conversion failed");
    let failed_result = FlowResult::Failed(error);

    store
        .update_step_status(
            run_id,
            0,
            "convert",
            1,
            StepStatus::Failed,
            Some("/mock/convert"),
            Some(failed_result.clone()),
            SequenceNumber::new(1),
        )
        .await
        .unwrap();

    let statuses = store
        .get_step_statuses(run_id, None, None)
        .await
        .expect("get_step_statuses with failed result should succeed");

    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].status, StepStatus::Failed);
    assert_eq!(statuses[0].result, Some(failed_result));
}

#[tokio::test]
async fn test_step_status_concurrent_read_write_file_backed() {
    use crate::SqliteStateStoreConfig;
    use std::sync::Arc;
    use stepflow_core::ValueExpr;
    use stepflow_core::status::StepStatus;
    use stepflow_core::workflow::{FlowBuilder, StepBuilder};
    use stepflow_state::{BlobStore as _, CreateRunParams, MetadataStore as _};

    // Use a file-backed SQLite database to exercise the same code path as production.
    // This specifically tests that PRAGMAs (journal_mode=WAL, busy_timeout) are set
    // on ALL pool connections, not just the first one.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    let store = SqliteStateStore::new(SqliteStateStoreConfig {
        database_url: db_url,
        max_connections: 10,
        auto_migrate: true,
    })
    .await
    .unwrap();

    let flow = Arc::new(
        FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::new("step1")
                    .component("/mock/test")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step1".to_string(),
                path: Default::default(),
            })
            .build(),
    );
    let flow_id = store.store_flow(flow).await.unwrap();

    let store = Arc::new(store);

    // Spawn concurrent writes and reads to exercise pool connection contention.
    let mut handles = Vec::new();
    for i in 0..10u32 {
        let store = store.clone();
        let flow_id = flow_id.clone();
        handles.push(tokio::spawn(async move {
            let run_id = Uuid::now_v7();
            let input = ValueRef::new(json!({"idx": i}));
            store
                .create_run(CreateRunParams::new(
                    run_id,
                    flow_id,
                    vec![input],
                    SequenceNumber::new(0),
                ))
                .await
                .unwrap();

            // Write step status
            let result = FlowResult::from(json!({"value": i}));
            store
                .update_step_status(
                    run_id,
                    0,
                    "step1",
                    0,
                    StepStatus::Completed,
                    Some("/mock/test"),
                    Some(result.clone()),
                    SequenceNumber::new(1),
                )
                .await
                .unwrap();

            // Read it back immediately (while other tasks are writing)
            let statuses = store
                .get_step_statuses(run_id, None, None)
                .await
                .expect("concurrent read should succeed");
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].result, Some(result));
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}
