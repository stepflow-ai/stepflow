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
// SQLite-specific step status tests (issue #849)
//
// Contract-level step status tests are in metadata_compliance.rs and run
// against all state store backends. The test below exercises SQLite-specific
// behavior: file-backed storage with concurrent pool connections, validating
// that PRAGMAs (journal_mode=WAL, busy_timeout) are applied to ALL pool
// connections.
// =========================================================================

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

// =========================================================================
// Schema drift test
//
// Exercises every query_as<_, FromRow>() against a freshly migrated schema.
// If a migration adds/removes/renames a column without a corresponding
// FromRow struct update, the query_as decode will fail → test fails.
// =========================================================================

#[tokio::test]
async fn test_schema_matches_from_row_structs() {
    use std::sync::Arc;
    use stepflow_core::ValueExpr;
    use stepflow_core::status::StepStatus;
    use stepflow_core::workflow::{FlowBuilder, StepBuilder, ValueRef};
    use stepflow_state::{
        BlobStore as _, CheckpointStore as _, CreateRunParams, ExecutionJournal as _, JournalEvent,
        MetadataStore as _,
    };

    let store = SqliteStateStore::in_memory().await.unwrap();

    // Seed data so every query has rows to decode
    let flow = Arc::new(
        FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::new("s1")
                    .component("/mock/test")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "s1".to_string(),
                path: Default::default(),
            })
            .build(),
    );

    // BlobRow: store and get a blob
    let blob_content = serde_json::to_vec(&json!({"test": true})).unwrap();
    let blob_id = store
        .put_blob(
            &blob_content,
            stepflow_core::BlobType::Data,
            Default::default(),
        )
        .await
        .unwrap();
    store
        .get_blob(&blob_id)
        .await
        .expect("BlobRow: get_blob should decode all columns")
        .expect("blob should exist");

    // RunRow + RunStatRow: create a run and get it
    let flow_id = store.store_flow(flow).await.unwrap();
    let run_id = Uuid::now_v7();
    store
        .create_run(CreateRunParams::new(
            run_id,
            flow_id,
            vec![ValueRef::new(json!({"x": 1}))],
            SequenceNumber::new(0),
        ))
        .await
        .unwrap();

    store
        .get_run(run_id)
        .await
        .expect("RunRow: get_run should decode all columns")
        .expect("run should exist");

    // ListRunsRow: list runs
    let runs = store
        .list_runs(&Default::default())
        .await
        .expect("ListRunsRow: list_runs should decode all columns");
    assert!(!runs.is_empty());

    // StepStatusRow: write and read step status
    store
        .update_step_status(
            run_id,
            0,
            "s1",
            0,
            StepStatus::Completed,
            Some("/mock/test"),
            Some(FlowResult::from(json!({"ok": true}))),
            SequenceNumber::new(1),
        )
        .await
        .unwrap();

    store
        .get_step_status(run_id, 0, "s1")
        .await
        .expect("StepStatusRow: get_step_status should decode all columns")
        .expect("step status should exist");

    store
        .get_step_statuses(run_id, None, None)
        .await
        .expect("StepStatusRow: get_step_statuses should decode all columns");

    // ItemResultRow: get item results
    store
        .get_item_results(run_id, stepflow_domain::ResultOrder::ByIndex)
        .await
        .expect("ItemResultRow: get_item_results should decode all columns");

    // JournalEntryRow + MaxSeqRow: write and read journal
    let root_run_id = run_id;
    let journal_flow_id =
        stepflow_core::BlobId::from_content(&ValueRef::new(json!({"test": "flow"}))).unwrap();
    store
        .write(
            root_run_id,
            JournalEvent::RootRunCreated {
                run_id,
                flow_id: journal_flow_id,
                inputs: vec![],
                variables: Default::default(),
            },
        )
        .await
        .expect("MaxSeqRow: write should decode max_seq column");

    // MaxSeqRow: latest_sequence
    store
        .latest_sequence(root_run_id)
        .await
        .expect("MaxSeqRow: latest_sequence should decode all columns");

    // RootJournalRow: list_active_roots
    store
        .list_active_roots()
        .await
        .expect("RootJournalRow: list_active_roots should decode all columns");

    // JournalEntryRow: stream entries
    use futures::StreamExt as _;
    let mut stream = store.stream_from(root_run_id, SequenceNumber::new(0));
    let entry = stream.next().await;
    assert!(
        entry.is_some(),
        "JournalEntryRow: stream should return at least one entry"
    );
    entry
        .unwrap()
        .expect("JournalEntryRow: stream_from should decode all columns");

    // CheckpointRow: store and get checkpoint
    store
        .put_checkpoint(
            root_run_id,
            SequenceNumber::new(1),
            bytes::Bytes::from_static(&[1, 2, 3]),
        )
        .await
        .unwrap();
    store
        .get_latest_checkpoint(root_run_id)
        .await
        .expect("CheckpointRow: get_latest_checkpoint should decode all columns")
        .expect("checkpoint should exist");
}
