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

use error_stack::{Result, ResultExt as _};
use sqlx::{Row as _, SqliteConnection, SqlitePool};
use stepflow_state::StateError;

/// Run a migration within a serialized transaction, rolling back on error.
///
/// Acquires a write lock via [`begin_migration`], executes `f`, records the
/// migration via [`complete_migration`], and commits. If `f` fails, the
/// transaction is explicitly rolled back before the error propagates, so the
/// connection is returned to the pool in a clean state.
async fn run_migration(
    pool: &SqlitePool,
    name: &str,
    f: impl AsyncFnOnce(&mut SqliteConnection) -> Result<(), StateError>,
) -> Result<(), StateError> {
    if let Some(mut conn) = begin_migration(pool, name).await? {
        match f(&mut conn).await {
            Ok(()) => complete_migration(&mut conn, name).await?,
            Err(e) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(e);
            }
        }
    }
    Ok(())
}

/// Run blob store migrations (creates the `blobs` table and indexes).
pub async fn run_blob_migrations(pool: &SqlitePool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;
    run_migration(pool, "001_create_blob_tables", create_blob_tables).await?;
    Ok(())
}

/// Run metadata store migrations (creates `runs`, `step_results`, `step_info`,
/// `run_items` tables and their indexes).
///
/// Metadata tables have no FK references to the blobs table because blobs may
/// live in a different backend (e.g., S3, filesystem).
pub async fn run_metadata_migrations(pool: &SqlitePool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;

    run_migration(pool, "001_create_metadata_tables", create_metadata_tables).await?;
    run_migration(pool, "003_add_step_statuses_to_run_items", add_step_statuses_column).await?;
    run_migration(pool, "004_add_orchestrator_id_to_runs", add_orchestrator_id_column).await?;

    Ok(())
}

/// Run journal migrations (creates the `journal_entries` table and indexes).
pub async fn run_journal_migrations(pool: &SqlitePool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;
    run_migration(pool, "002_create_journal_tables", create_journal_tables).await?;
    Ok(())
}

/// Run checkpoint migrations (creates the `checkpoints` table).
pub async fn run_checkpoint_migrations(pool: &SqlitePool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;
    run_migration(pool, "005_create_checkpoint_table", create_checkpoint_table).await?;
    Ok(())
}

/// Run all migrations (blob + metadata + journal + checkpoint). Convenience for tests and
/// single-instance deployments where one SQLite database backs all stores.
pub async fn run_all_migrations(pool: &SqlitePool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;

    run_blob_migrations(pool).await?;
    run_metadata_migrations(pool).await?;
    run_journal_migrations(pool).await?;
    run_checkpoint_migrations(pool).await?;

    Ok(())
}

/// Create the migrations tracking table
async fn create_migrations_table(pool: &SqlitePool) -> Result<(), StateError> {
    let sql = r#"
        CREATE TABLE IF NOT EXISTS _stepflow_migrations (
            name TEXT PRIMARY KEY,
            applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
    "#;

    sqlx::query(sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Apply a migration if it hasn't been applied yet.
///
/// Performs an optimistic read-only check first (no lock), then, if the migration
/// needs to run, starts a write transaction using `BEGIN IMMEDIATE` so that a
/// reserved write lock is held only while applying the migration. This keeps the
/// common case (already migrated) lock-free and minimises write-lock duration.
///
/// Callers must execute the migration on the returned connection and then call
/// [`complete_migration`] to record it and commit. On error, callers should
/// `ROLLBACK` before dropping the connection — use [`run_migration`] which
/// handles this automatically.
async fn begin_migration(
    pool: &SqlitePool,
    name: &str,
) -> Result<Option<sqlx::pool::PoolConnection<sqlx::Sqlite>>, StateError> {
    // Optimistic check — no write lock needed when migrations are already applied.
    let row = sqlx::query("SELECT COUNT(*) as count FROM _stepflow_migrations WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .change_context(StateError::Initialization)?;

    let count: i64 = row
        .try_get("count")
        .change_context(StateError::Initialization)?;

    if count > 0 {
        return Ok(None);
    }

    // Migration appears to be needed — acquire a connection and take a write lock
    // so concurrent callers are serialized.
    let mut conn = pool
        .acquire()
        .await
        .change_context(StateError::Initialization)?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    // Re-check under the write lock in case another process applied it first.
    let row = sqlx::query("SELECT COUNT(*) as count FROM _stepflow_migrations WHERE name = ?")
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    let count: i64 = row
        .try_get("count")
        .change_context(StateError::Initialization)?;

    if count > 0 {
        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .change_context(StateError::Initialization)?;
        return Ok(None);
    }

    Ok(Some(conn))
}

/// Record that a migration was applied and commit the transaction.
async fn complete_migration(conn: &mut SqliteConnection, name: &str) -> Result<(), StateError> {
    sqlx::query("INSERT OR IGNORE INTO _stepflow_migrations (name) VALUES (?)")
        .bind(name)
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Create the blob tables for content-addressable storage.
async fn create_blob_tables(conn: &mut SqliteConnection) -> Result<(), StateError> {
    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS blobs (
                id TEXT PRIMARY KEY,
                data BLOB NOT NULL,
                blob_type TEXT NOT NULL DEFAULT 'data',
                filename TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
        "#,
    )
    .execute(&mut *conn)
    .await
    .change_context(StateError::Initialization)?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_blobs_type ON blobs(blob_type)")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Create the metadata tables for runs, steps, and run items.
async fn create_metadata_tables(conn: &mut SqliteConnection) -> Result<(), StateError> {
    let table_commands = vec![
        // Runs table with flow metadata and hierarchy support for sub-flows.
        // flow_id is a blob reference but has no FK constraint because blobs
        // may be stored in a different backend (e.g., S3, filesystem).
        r#"
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                flow_id TEXT,
                flow_name TEXT,
                status TEXT DEFAULT 'running',
                overrides_json TEXT,
                root_run_id TEXT NOT NULL,
                parent_run_id TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                completed_at DATETIME
            )
        "#,
        // Step results table for flow step execution results
        r#"
            CREATE TABLE IF NOT EXISTS step_results (
                run_id TEXT NOT NULL,
                step_index INTEGER NOT NULL,
                step_id TEXT,
                result TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (run_id, step_index),
                FOREIGN KEY (run_id) REFERENCES runs(id)
            )
        "#,
        // Step info table for tracking step execution metadata
        r#"
            CREATE TABLE IF NOT EXISTS step_info (
                run_id TEXT NOT NULL,
                step_index INTEGER NOT NULL,
                step_id TEXT NOT NULL,
                component TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (run_id, step_index),
                FOREIGN KEY (run_id) REFERENCES runs(id)
            )
        "#,
        // Run items table for multi-item runs (input and result per item)
        r#"
            CREATE TABLE IF NOT EXISTS run_items (
                run_id TEXT NOT NULL,
                item_index INTEGER NOT NULL,
                input_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'running',
                result_json TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                completed_at DATETIME,
                PRIMARY KEY (run_id, item_index),
                FOREIGN KEY (run_id) REFERENCES runs(id) ON DELETE CASCADE
            )
        "#,
    ];

    for sql in table_commands {
        sqlx::query(sql)
            .execute(&mut *conn)
            .await
            .change_context(StateError::Initialization)?;
    }

    let index_commands = vec![
        // Step results indexes
        "CREATE INDEX IF NOT EXISTS idx_step_results_step_id ON step_results(run_id, step_id)",
        // Step info indexes
        "CREATE INDEX IF NOT EXISTS idx_step_info_run_id ON step_info(run_id)",
        "CREATE INDEX IF NOT EXISTS idx_step_info_status ON step_info(run_id, status)",
        // Run indexes
        "CREATE INDEX IF NOT EXISTS idx_runs_flow_id ON runs(flow_id)",
        "CREATE INDEX IF NOT EXISTS idx_runs_status ON runs(status)",
        "CREATE INDEX IF NOT EXISTS idx_runs_created_at ON runs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_runs_root_run_id ON runs(root_run_id)",
        "CREATE INDEX IF NOT EXISTS idx_runs_parent_run_id ON runs(parent_run_id)",
        // Run items indexes
        "CREATE INDEX IF NOT EXISTS idx_run_items_run_id ON run_items(run_id)",
        "CREATE INDEX IF NOT EXISTS idx_run_items_status ON run_items(run_id, status)",
    ];

    for sql in index_commands {
        sqlx::query(sql)
            .execute(&mut *conn)
            .await
            .change_context(StateError::Initialization)?;
    }

    Ok(())
}

/// Create journal tables for write-ahead logging and recovery.
///
/// Journals are keyed by `root_run_id`, meaning all events for an execution tree
/// (parent flow + all subflows) share a single journal with a unified sequence space.
/// Each entry contains a `run_id` to identify which specific run the event belongs to.
async fn create_journal_tables(conn: &mut SqliteConnection) -> Result<(), StateError> {
    // Journal entries for recovery - stores execution events.
    // Keyed by (root_run_id, sequence) so all events for an execution tree
    // share one journal with monotonically increasing sequence numbers.
    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS journal_entries (
                root_run_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                run_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                event_data TEXT NOT NULL,
                PRIMARY KEY (root_run_id, sequence)
            )
        "#,
    )
    .execute(&mut *conn)
    .await
    .change_context(StateError::Initialization)?;

    // Index for filtering by specific run_id within a journal
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_journal_run_id ON journal_entries(run_id)")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Add step_statuses_json column to run_items table.
///
/// This stores a JSON array of step status info for each completed item,
/// allowing step-level status to be queried without accessing the journal.
async fn add_step_statuses_column(conn: &mut SqliteConnection) -> Result<(), StateError> {
    sqlx::query("ALTER TABLE run_items ADD COLUMN step_statuses_json TEXT")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Add orchestrator_id column to runs table for multi-orchestrator recovery.
///
/// Tracks which orchestrator owns each run, enabling targeted recovery queries.
/// NULL means the run is orphaned (available for claiming).
async fn add_orchestrator_id_column(conn: &mut SqliteConnection) -> Result<(), StateError> {
    sqlx::query("ALTER TABLE runs ADD COLUMN orchestrator_id TEXT")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_runs_orchestrator_id ON runs(orchestrator_id)")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Create the checkpoints table for periodic execution state snapshots.
///
/// Checkpoints are keyed by `root_run_id`. Only the latest checkpoint per root
/// run is needed; the table uses INSERT OR REPLACE semantics to keep just one row.
async fn create_checkpoint_table(conn: &mut SqliteConnection) -> Result<(), StateError> {
    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS checkpoints (
                root_run_id TEXT NOT NULL PRIMARY KEY,
                sequence_number INTEGER NOT NULL,
                data BLOB NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
    )
    .execute(&mut *conn)
    .await
    .change_context(StateError::Initialization)?;

    Ok(())
}
