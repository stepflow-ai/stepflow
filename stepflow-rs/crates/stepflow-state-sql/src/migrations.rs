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
use sqlx::{Row as _, SqlitePool};
use stepflow_state::StateError;

/// Run migrations to set up the database schema
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), StateError> {
    // Create migration tracking table first
    create_migrations_table(pool).await?;

    // Apply the unified schema migration
    apply_migration(pool, "001_create_unified_schema", || {
        create_unified_schema(pool)
    })
    .await?;

    // Apply batch execution migration
    apply_migration(pool, "002_add_batch_execution", || {
        add_batch_execution_tables(pool)
    })
    .await?;

    // Apply overrides column migration
    apply_migration(pool, "003_add_overrides_column", || {
        add_overrides_column(pool)
    })
    .await?;

    // Apply debug queue table migration
    apply_migration(pool, "004_add_debug_queue", || add_debug_queue_table(pool)).await?;

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

/// Apply a migration if it hasn't been applied yet
async fn apply_migration<F, Fut>(
    pool: &SqlitePool,
    name: &str,
    migration_fn: F,
) -> Result<(), StateError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), StateError>>,
{
    // Check if migration has already been applied
    let row = sqlx::query("SELECT COUNT(*) as count FROM _stepflow_migrations WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .change_context(StateError::Initialization)?;

    let count: i64 = row
        .try_get("count")
        .change_context(StateError::Initialization)?;

    if count > 0 {
        // Migration already applied
        return Ok(());
    }

    // Apply the migration
    migration_fn().await?;

    // Record that migration was applied
    sqlx::query("INSERT INTO _stepflow_migrations (name) VALUES (?)")
        .bind(name)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Create the unified database schema in one migration
async fn create_unified_schema(pool: &SqlitePool) -> Result<(), StateError> {
    // Create all tables with their final schema
    let table_commands = vec![
        // Unified blobs table for content-addressable storage (both data and flows)
        r#"
            CREATE TABLE IF NOT EXISTS blobs (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                blob_type TEXT NOT NULL DEFAULT 'data',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
        "#,
        // Runs table with flow metadata
        r#"
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                flow_id TEXT,
                flow_name TEXT,        -- from flow.name field for display
                flow_label TEXT,       -- label used for execution (if any)
                status TEXT DEFAULT 'running',
                debug_mode BOOLEAN DEFAULT FALSE,
                input_json TEXT,
                result_json TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                completed_at DATETIME,
                FOREIGN KEY (flow_id) REFERENCES blobs(id)
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
        // Flow labels table for named flow versions
        r#"
            CREATE TABLE IF NOT EXISTS flow_labels (
                name TEXT NOT NULL,  -- from flow.name field
                label TEXT NOT NULL, -- like "production", "staging", "latest"
                flow_id TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (name, label),
                FOREIGN KEY (flow_id) REFERENCES blobs(id)
            )
        "#,
    ];

    // Execute table creation commands
    for sql in table_commands {
        sqlx::query(sql)
            .execute(pool)
            .await
            .change_context(StateError::Initialization)?;
    }

    // Create all indexes for optimal performance
    let index_commands = vec![
        // Blob indexes
        "CREATE INDEX IF NOT EXISTS idx_blobs_type ON blobs(blob_type)",
        // Step results indexes
        "CREATE INDEX IF NOT EXISTS idx_step_results_step_id ON step_results(run_id, step_id)",
        // Step info indexes
        "CREATE INDEX IF NOT EXISTS idx_step_info_run_id ON step_info(run_id)",
        "CREATE INDEX IF NOT EXISTS idx_step_info_status ON step_info(run_id, status)",
        // Run indexes
        "CREATE INDEX IF NOT EXISTS idx_runs_flow_id ON runs(flow_id)",
        "CREATE INDEX IF NOT EXISTS idx_runs_status ON runs(status)",
        "CREATE INDEX IF NOT EXISTS idx_runs_created_at ON runs(created_at)",
        // Workflow labels indexes
        "CREATE INDEX IF NOT EXISTS idx_flow_labels_name ON flow_labels(name)",
        "CREATE INDEX IF NOT EXISTS idx_flow_labels_flow_id ON flow_labels(flow_id)",
        "CREATE INDEX IF NOT EXISTS idx_flow_labels_created_at ON flow_labels(created_at)",
    ];

    // Execute index creation commands
    for sql in index_commands {
        sqlx::query(sql)
            .execute(pool)
            .await
            .change_context(StateError::Initialization)?;
    }

    Ok(())
}

/// Add batch execution tables
async fn add_batch_execution_tables(pool: &SqlitePool) -> Result<(), StateError> {
    let table_commands = vec![
        // Batches table for storing batch metadata
        r#"
            CREATE TABLE IF NOT EXISTS batches (
                id TEXT PRIMARY KEY,
                flow_id TEXT NOT NULL,
                flow_name TEXT,
                total_inputs INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'running',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (flow_id) REFERENCES blobs(id)
            )
        "#,
        // Batch runs join table for linking batches to runs
        r#"
            CREATE TABLE IF NOT EXISTS batch_runs (
                batch_id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                batch_input_index INTEGER NOT NULL,
                PRIMARY KEY (batch_id, run_id),
                FOREIGN KEY (batch_id) REFERENCES batches(id),
                FOREIGN KEY (run_id) REFERENCES runs(id)
            )
        "#,
    ];

    // Execute table creation commands
    for sql in table_commands {
        sqlx::query(sql)
            .execute(pool)
            .await
            .change_context(StateError::Initialization)?;
    }

    // Create indexes for batch queries
    let index_commands = vec![
        // Batch indexes for efficient queries
        "CREATE INDEX IF NOT EXISTS idx_batches_flow_name ON batches(flow_name)",
        "CREATE INDEX IF NOT EXISTS idx_batches_status ON batches(status)",
        "CREATE INDEX IF NOT EXISTS idx_batches_created_at ON batches(created_at)",
        // Batch runs indexes for efficient joins and reverse lookups
        "CREATE INDEX IF NOT EXISTS idx_batch_runs_run_id ON batch_runs(run_id)",
        "CREATE INDEX IF NOT EXISTS idx_batch_runs_batch_id ON batch_runs(batch_id, batch_input_index)",
    ];

    // Execute index creation commands
    for sql in index_commands {
        sqlx::query(sql)
            .execute(pool)
            .await
            .change_context(StateError::Initialization)?;
    }

    Ok(())
}

/// Add overrides column to runs table
async fn add_overrides_column(pool: &SqlitePool) -> Result<(), StateError> {
    let sql = "ALTER TABLE runs ADD COLUMN overrides_json TEXT";

    sqlx::query(sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Add debug_queue table for persisting debug session queue
/// Uses a normalized structure with one row per step for efficient add/remove
async fn add_debug_queue_table(pool: &SqlitePool) -> Result<(), StateError> {
    let sql = r#"
        CREATE TABLE IF NOT EXISTS debug_queue (
            run_id TEXT NOT NULL,
            step_id TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (run_id, step_id),
            FOREIGN KEY (run_id) REFERENCES runs(id) ON DELETE CASCADE
        )
    "#;

    sqlx::query(sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    // Add index for efficient lookups by run_id
    let index_sql = "CREATE INDEX IF NOT EXISTS idx_debug_queue_run_id ON debug_queue(run_id)";
    sqlx::query(index_sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}
