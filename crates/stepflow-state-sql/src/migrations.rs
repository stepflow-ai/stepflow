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
        // Executions table with all metadata columns
        r#"
            CREATE TABLE IF NOT EXISTS executions (
                id TEXT PRIMARY KEY,
                endpoint_name TEXT,
                endpoint_label TEXT,
                workflow_hash TEXT,
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
        // Unified endpoints table with composite primary key
        r#"
            CREATE TABLE IF NOT EXISTS endpoints (
                name TEXT NOT NULL,
                label TEXT, -- NULL represents the default version
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
        "CREATE INDEX IF NOT EXISTS idx_executions_endpoint_name ON executions(endpoint_name)",
        "CREATE INDEX IF NOT EXISTS idx_executions_endpoint_label ON executions(endpoint_label)",
        "CREATE INDEX IF NOT EXISTS idx_executions_workflow_hash ON executions(workflow_hash)",
        "CREATE INDEX IF NOT EXISTS idx_executions_status ON executions(status)",
        "CREATE INDEX IF NOT EXISTS idx_executions_created_at ON executions(created_at)",
        // Endpoints indexes
        "CREATE INDEX IF NOT EXISTS idx_endpoints_name ON endpoints(name)",
        "CREATE INDEX IF NOT EXISTS idx_endpoints_workflow_hash ON endpoints(workflow_hash)",
        "CREATE INDEX IF NOT EXISTS idx_endpoints_created_at ON endpoints(created_at)",
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

