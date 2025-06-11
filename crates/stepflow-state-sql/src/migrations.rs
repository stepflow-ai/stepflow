use error_stack::{Result, ResultExt};
use sqlx::{Row, SqlitePool};
use stepflow_state::StateError;

/// Run all migrations to set up the database schema
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), StateError> {
    // Create migration tracking table first
    create_migrations_table(pool).await?;

    // Apply migrations in order
    apply_migration(pool, "001_create_blobs_table", || create_blobs_table(pool)).await?;
    apply_migration(pool, "002_create_executions_table", || {
        create_executions_table(pool)
    })
    .await?;
    apply_migration(pool, "003_create_step_results_table", || {
        create_step_results_table(pool)
    })
    .await?;
    apply_migration(pool, "004_create_indexes", || create_indexes(pool)).await?;
    apply_migration(pool, "005_create_workflows_table", || {
        create_workflows_table(pool)
    })
    .await?;
    apply_migration(pool, "006_create_unified_endpoints_table", || {
        create_unified_endpoints_table(pool)
    })
    .await?;
    apply_migration(pool, "007_enhance_executions_table", || {
        enhance_executions_table(pool)
    })
    .await?;

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

/// Create blobs table for content-addressable storage
async fn create_blobs_table(pool: &SqlitePool) -> Result<(), StateError> {
    let sql = r#"
        CREATE TABLE blobs (
            id TEXT PRIMARY KEY,
            data TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
    "#;

    sqlx::query(sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Create executions table for workflow execution tracking
async fn create_executions_table(pool: &SqlitePool) -> Result<(), StateError> {
    let sql = r#"
        CREATE TABLE executions (
            id TEXT PRIMARY KEY,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
    "#;

    sqlx::query(sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Create step_results table for workflow step execution results
async fn create_step_results_table(pool: &SqlitePool) -> Result<(), StateError> {
    let sql = r#"
        CREATE TABLE step_results (
            execution_id TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            step_id TEXT,
            result TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (execution_id, step_index)
        )
    "#;

    sqlx::query(sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Create indexes for performance
async fn create_indexes(pool: &SqlitePool) -> Result<(), StateError> {
    let sql = r#"
        CREATE INDEX idx_step_results_step_id 
        ON step_results(execution_id, step_id)
    "#;

    sqlx::query(sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Create workflows table for content-addressable workflow storage
async fn create_workflows_table(pool: &SqlitePool) -> Result<(), StateError> {
    let sql = r#"
        CREATE TABLE workflows (
            hash TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            first_seen DATETIME DEFAULT CURRENT_TIMESTAMP
        )
    "#;

    sqlx::query(sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    Ok(())
}

/// Create unified endpoints table with composite primary key
async fn create_unified_endpoints_table(pool: &SqlitePool) -> Result<(), StateError> {
    let sql = r#"
        CREATE TABLE endpoints (
            name TEXT NOT NULL,
            label TEXT, -- NULL represents the default version
            workflow_hash TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (name, label),
            FOREIGN KEY (workflow_hash) REFERENCES workflows(hash)
        )
    "#;

    sqlx::query(sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    // Create indexes for efficient querying
    let index_commands = vec![
        "CREATE INDEX idx_endpoints_name ON endpoints(name)",
        "CREATE INDEX idx_endpoints_workflow_hash ON endpoints(workflow_hash)",
        "CREATE INDEX idx_endpoints_created_at ON endpoints(created_at)",
    ];

    for sql in index_commands {
        sqlx::query(sql)
            .execute(pool)
            .await
            .change_context(StateError::Initialization)?;
    }

    Ok(())
}

/// Enhance executions table with additional metadata columns
async fn enhance_executions_table(pool: &SqlitePool) -> Result<(), StateError> {
    // Add new columns to the executions table
    let sql_commands = vec![
        "ALTER TABLE executions ADD COLUMN endpoint_name TEXT",
        "ALTER TABLE executions ADD COLUMN workflow_hash TEXT",
        "ALTER TABLE executions ADD COLUMN status TEXT DEFAULT 'running'",
        "ALTER TABLE executions ADD COLUMN debug_mode BOOLEAN DEFAULT FALSE",
        "ALTER TABLE executions ADD COLUMN input_blob_id TEXT",
        "ALTER TABLE executions ADD COLUMN result_blob_id TEXT",
        "ALTER TABLE executions ADD COLUMN completed_at DATETIME",
    ];

    for sql in sql_commands {
        sqlx::query(sql)
            .execute(pool)
            .await
            .change_context(StateError::Initialization)?;
    }

    // Create indexes for the new columns
    let index_commands = vec![
        "CREATE INDEX idx_executions_endpoint_name ON executions(endpoint_name)",
        "CREATE INDEX idx_executions_workflow_hash ON executions(workflow_hash)",
        "CREATE INDEX idx_executions_status ON executions(status)",
        "CREATE INDEX idx_executions_created_at ON executions(created_at)",
    ];

    for sql in index_commands {
        sqlx::query(sql)
            .execute(pool)
            .await
            .change_context(StateError::Initialization)?;
    }

    Ok(())
}
