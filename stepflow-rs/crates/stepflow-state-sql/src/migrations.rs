use error_stack::{Result, ResultExt as _};
use sqlx::{Row as _, SqlitePool};
use stepflow_state::StateError;

/// Run migrations to set up the database schema
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), StateError> {
    // Create migration tracking table first
    create_migrations_table(pool).await?;

    // Apply the collapsed schema migration
    apply_migration(pool, "001_create_initial_schema", || {
        create_complete_schema(pool)
    })
    .await?;

    // Apply the step info migration
    apply_migration(pool, "002_add_step_info", || add_step_info_table(pool)).await?;

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

/// Create the complete database schema in one migration
async fn create_complete_schema(pool: &SqlitePool) -> Result<(), StateError> {
    // Create all tables with their final schema
    let table_commands = vec![
        // Blobs table for content-addressable storage
        r#"
            CREATE TABLE IF NOT EXISTS blobs (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
        "#,
        // Workflows table for content-addressable workflow storage
        r#"
            CREATE TABLE IF NOT EXISTS workflows (
                hash TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                first_seen DATETIME DEFAULT CURRENT_TIMESTAMP
            )
        "#,
        // Executions table with workflow metadata
        r#"
            CREATE TABLE IF NOT EXISTS executions (
                id TEXT PRIMARY KEY,
                workflow_hash TEXT,
                workflow_name TEXT,        -- from workflow.name field for display
                workflow_label TEXT,       -- label used for execution (if any)
                status TEXT DEFAULT 'running',
                debug_mode BOOLEAN DEFAULT FALSE,
                input_json TEXT,
                result_json TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                completed_at DATETIME,
                FOREIGN KEY (workflow_hash) REFERENCES workflows(hash)
            )
        "#,
        // Step results table for workflow step execution results
        r#"
            CREATE TABLE IF NOT EXISTS step_results (
                execution_id TEXT NOT NULL,
                step_index INTEGER NOT NULL,
                step_id TEXT,
                result TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (execution_id, step_index),
                FOREIGN KEY (execution_id) REFERENCES executions(id)
            )
        "#,
        // Workflow labels table for named workflow versions
        r#"
            CREATE TABLE IF NOT EXISTS workflow_labels (
                name TEXT NOT NULL,  -- from workflow.name field
                label TEXT NOT NULL, -- like "production", "staging", "latest"
                workflow_hash TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (name, label),
                FOREIGN KEY (workflow_hash) REFERENCES workflows(hash)
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
        // Step results indexes
        "CREATE INDEX IF NOT EXISTS idx_step_results_step_id ON step_results(execution_id, step_id)",
        // Executions indexes
        "CREATE INDEX IF NOT EXISTS idx_executions_workflow_hash ON executions(workflow_hash)",
        "CREATE INDEX IF NOT EXISTS idx_executions_status ON executions(status)",
        "CREATE INDEX IF NOT EXISTS idx_executions_created_at ON executions(created_at)",
        // Workflow labels indexes
        "CREATE INDEX IF NOT EXISTS idx_workflow_labels_name ON workflow_labels(name)",
        "CREATE INDEX IF NOT EXISTS idx_workflow_labels_workflow_hash ON workflow_labels(workflow_hash)",
        "CREATE INDEX IF NOT EXISTS idx_workflow_labels_created_at ON workflow_labels(created_at)",
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

/// Create step info table
async fn add_step_info_table(pool: &SqlitePool) -> Result<(), StateError> {
    // Create step info table
    let step_info_sql = r#"
        CREATE TABLE IF NOT EXISTS step_info (
            execution_id TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            step_id TEXT NOT NULL,
            component TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (execution_id, step_index),
            FOREIGN KEY (execution_id) REFERENCES executions(id)
        )
    "#;

    sqlx::query(step_info_sql)
        .execute(pool)
        .await
        .change_context(StateError::Initialization)?;

    // Create indexes for performance
    let index_commands = vec![
        "CREATE INDEX IF NOT EXISTS idx_step_info_execution_id ON step_info(execution_id)",
        "CREATE INDEX IF NOT EXISTS idx_step_info_status ON step_info(execution_id, status)",
    ];

    for sql in index_commands {
        sqlx::query(sql)
            .execute(pool)
            .await
            .change_context(StateError::Initialization)?;
    }

    Ok(())
}
