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

//! SQLite-specific DDL migrations.
//!
//! Uses `BEGIN IMMEDIATE` for write-lock serialization and SQLite-specific
//! types (`DATETIME`, `BLOB`).

use error_stack::{Result, ResultExt as _};
use sqlx::Row as _;
use sqlx::{AnyConnection, AnyPool};
use stepflow_state::StateError;

/// Run a migration within a serialized transaction, rolling back on error.
async fn run_migration(
    pool: &AnyPool,
    name: &str,
    f: impl AsyncFnOnce(&mut AnyConnection) -> Result<(), StateError>,
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
pub async fn run_blob_migrations(pool: &AnyPool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;
    run_migration(pool, "001_create_blob_tables", create_blob_tables).await?;
    Ok(())
}

/// Run metadata store migrations.
pub async fn run_metadata_migrations(pool: &AnyPool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;

    run_migration(pool, "001_create_metadata_tables", create_metadata_tables).await?;
    run_migration(
        pool,
        "003_add_step_statuses_to_run_items",
        add_step_statuses_column,
    )
    .await?;
    run_migration(
        pool,
        "004_add_orchestrator_id_to_runs",
        add_orchestrator_id_column,
    )
    .await?;
    run_migration(
        pool,
        "005_add_created_at_seqno_to_runs",
        add_created_at_seqno_column,
    )
    .await?;
    run_migration(
        pool,
        "006_add_finished_at_seqno_to_runs",
        add_finished_at_seqno_column,
    )
    .await?;
    run_migration(
        pool,
        "007_create_step_statuses_table",
        create_step_statuses_table,
    )
    .await?;

    run_migration(
        pool,
        "008_create_component_registrations_table",
        create_component_registrations_table,
    )
    .await?;

    Ok(())
}

/// Run journal migrations (creates the `journal_entries` table and indexes).
pub async fn run_journal_migrations(pool: &AnyPool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;
    run_migration(pool, "002_create_journal_tables", create_journal_tables).await?;
    Ok(())
}

/// Run checkpoint migrations (creates the `checkpoints` table).
pub async fn run_checkpoint_migrations(pool: &AnyPool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;
    run_migration(pool, "005_create_checkpoint_table", create_checkpoint_table).await?;
    Ok(())
}

/// Run all migrations. Convenience for tests and single-instance deployments.
pub async fn run_all_migrations(pool: &AnyPool) -> Result<(), StateError> {
    create_migrations_table(pool).await?;
    run_blob_migrations(pool).await?;
    run_metadata_migrations(pool).await?;
    run_journal_migrations(pool).await?;
    run_checkpoint_migrations(pool).await?;
    Ok(())
}

/// Create the migrations tracking table.
async fn create_migrations_table(pool: &AnyPool) -> Result<(), StateError> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS _stepflow_migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT DEFAULT CURRENT_TIMESTAMP
        )
    "#,
    )
    .execute(pool)
    .await
    .change_context(StateError::Initialization)?;
    Ok(())
}

/// Apply a migration if it hasn't been applied yet.
///
/// Uses `BEGIN IMMEDIATE` to acquire an exclusive write lock (SQLite-specific).
async fn begin_migration(
    pool: &AnyPool,
    name: &str,
) -> Result<Option<sqlx::pool::PoolConnection<sqlx::Any>>, StateError> {
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

    let mut conn = pool
        .acquire()
        .await
        .change_context(StateError::Initialization)?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    // Re-check under the write lock.
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
async fn complete_migration(conn: &mut AnyConnection, name: &str) -> Result<(), StateError> {
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

async fn create_blob_tables(conn: &mut AnyConnection) -> Result<(), StateError> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS blobs (
            id TEXT PRIMARY KEY,
            data BLOB NOT NULL,
            blob_type TEXT NOT NULL DEFAULT 'data',
            filename TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
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

async fn create_metadata_tables(conn: &mut AnyConnection) -> Result<(), StateError> {
    let table_commands = vec![
        r#"
        CREATE TABLE IF NOT EXISTS runs (
            id TEXT PRIMARY KEY,
            flow_id TEXT,
            flow_name TEXT,
            status TEXT DEFAULT 'running',
            overrides_json TEXT,
            root_run_id TEXT NOT NULL,
            parent_run_id TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            completed_at TEXT
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS step_results (
            run_id TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            step_id TEXT,
            result TEXT NOT NULL,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (run_id, step_index),
            FOREIGN KEY (run_id) REFERENCES runs(id)
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS step_info (
            run_id TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            step_id TEXT NOT NULL,
            component TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (run_id, step_index),
            FOREIGN KEY (run_id) REFERENCES runs(id)
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS run_items (
            run_id TEXT NOT NULL,
            item_index INTEGER NOT NULL,
            input_json TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'running',
            result_json TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            completed_at TEXT,
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
        "CREATE INDEX IF NOT EXISTS idx_step_results_step_id ON step_results(run_id, step_id)",
        "CREATE INDEX IF NOT EXISTS idx_step_info_run_id ON step_info(run_id)",
        "CREATE INDEX IF NOT EXISTS idx_step_info_status ON step_info(run_id, status)",
        "CREATE INDEX IF NOT EXISTS idx_runs_flow_id ON runs(flow_id)",
        "CREATE INDEX IF NOT EXISTS idx_runs_status ON runs(status)",
        "CREATE INDEX IF NOT EXISTS idx_runs_created_at ON runs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_runs_root_run_id ON runs(root_run_id)",
        "CREATE INDEX IF NOT EXISTS idx_runs_parent_run_id ON runs(parent_run_id)",
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

async fn create_journal_tables(conn: &mut AnyConnection) -> Result<(), StateError> {
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

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_journal_run_id ON journal_entries(run_id)")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;
    Ok(())
}

async fn add_step_statuses_column(conn: &mut AnyConnection) -> Result<(), StateError> {
    sqlx::query("ALTER TABLE run_items ADD COLUMN step_statuses_json TEXT")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;
    Ok(())
}

async fn add_orchestrator_id_column(conn: &mut AnyConnection) -> Result<(), StateError> {
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

async fn add_created_at_seqno_column(conn: &mut AnyConnection) -> Result<(), StateError> {
    sqlx::query("ALTER TABLE runs ADD COLUMN created_at_seqno INTEGER")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_runs_root_run_id_created_at_seqno \
         ON runs(root_run_id, created_at_seqno)",
    )
    .execute(&mut *conn)
    .await
    .change_context(StateError::Initialization)?;
    Ok(())
}

async fn add_finished_at_seqno_column(conn: &mut AnyConnection) -> Result<(), StateError> {
    sqlx::query("ALTER TABLE runs ADD COLUMN finished_at_seqno INTEGER")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_runs_root_run_id_finished_at_seqno \
         ON runs(root_run_id, finished_at_seqno)",
    )
    .execute(&mut *conn)
    .await
    .change_context(StateError::Initialization)?;
    Ok(())
}

async fn create_step_statuses_table(conn: &mut AnyConnection) -> Result<(), StateError> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS step_statuses (
            run_id TEXT NOT NULL,
            item_index INTEGER NOT NULL,
            step_id TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            status TEXT NOT NULL,
            component TEXT,
            result_json TEXT,
            journal_seqno INTEGER NOT NULL,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (run_id, item_index, step_id),
            FOREIGN KEY (run_id) REFERENCES runs(id) ON DELETE CASCADE
        )
    "#,
    )
    .execute(&mut *conn)
    .await
    .change_context(StateError::Initialization)?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_step_statuses_run_id ON step_statuses(run_id)")
        .execute(&mut *conn)
        .await
        .change_context(StateError::Initialization)?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_step_statuses_journal_seqno \
         ON step_statuses(run_id, journal_seqno)",
    )
    .execute(&mut *conn)
    .await
    .change_context(StateError::Initialization)?;
    Ok(())
}

async fn create_checkpoint_table(conn: &mut AnyConnection) -> Result<(), StateError> {
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

/// Create the component_registrations table for persistent component metadata.
async fn create_component_registrations_table(conn: &mut AnyConnection) -> Result<(), StateError> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS component_registrations (
            plugin TEXT NOT NULL,
            component_id TEXT NOT NULL,
            path TEXT NOT NULL DEFAULT '',
            description TEXT,
            input_schema_json TEXT,
            output_schema_json TEXT,
            last_updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (plugin, component_id)
        )
    "#,
    )
    .execute(&mut *conn)
    .await
    .change_context(StateError::Initialization)?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_comp_reg_plugin ON component_registrations(plugin)",
    )
    .execute(&mut *conn)
    .await
    .change_context(StateError::Initialization)?;

    Ok(())
}
