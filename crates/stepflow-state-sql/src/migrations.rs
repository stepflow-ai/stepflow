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
