use std::future::Future;
use std::pin::Pin;

use error_stack::{Result, ResultExt};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use stepflow_core::{blob::BlobId, workflow::ValueRef, FlowResult};
use stepflow_state::{StateError, StateStore, StepResult};
use uuid::Uuid;

use crate::migrations;

/// Configuration for SqliteStateStore
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SqliteStateStoreConfig {
    pub database_url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_auto_migrate")]
    pub auto_migrate: bool,
}

fn default_max_connections() -> u32 {
    10
}

fn default_auto_migrate() -> bool {
    true
}

/// SQLite-based StateStore implementation
pub struct SqliteStateStore {
    pool: SqlitePool,
}

impl SqliteStateStore {
    /// Create a new SqliteStateStore with the given configuration
    pub async fn new(config: SqliteStateStoreConfig) -> Result<Self, StateError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.database_url)
            .await
            .change_context(StateError::Connection)?;

        if config.auto_migrate {
            migrations::run_migrations(&pool).await?;
        }

        Ok(Self { pool })
    }

    /// Create SqliteStateStore directly from a database URL
    pub async fn from_url(database_url: &str) -> Result<Self, StateError> {
        let config = SqliteStateStoreConfig {
            database_url: database_url.to_string(),
            max_connections: default_max_connections(),
            auto_migrate: default_auto_migrate(),
        };
        Self::new(config).await
    }

    /// Create an in-memory SQLite database for testing
    pub async fn in_memory() -> Result<Self, StateError> {
        Self::from_url("sqlite::memory:").await
    }

    /// Ensure an execution record exists
    async fn ensure_execution_exists(&self, execution_id: Uuid) -> Result<(), StateError> {
        let sql = "INSERT OR IGNORE INTO executions (id) VALUES (?)";

        sqlx::query(sql)
            .bind(execution_id.to_string())
            .execute(&self.pool)
            .await
            .change_context(StateError::Internal)?;

        Ok(())
    }
}

impl StateStore for SqliteStateStore {
    fn put_blob(
        &self,
        data: ValueRef,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<BlobId, StateError>> + Send + '_>> {
        Box::pin(async move {
            // Generate content-based ID
            let blob_id = BlobId::from_content(&data).change_context(StateError::Internal)?;

            // Serialize data to JSON string
            let json_str =
                serde_json::to_string(data.as_ref()).change_context(StateError::Serialization)?;

            // Simple INSERT OR IGNORE approach
            let sql = "INSERT OR IGNORE INTO blobs (id, data) VALUES (?, ?)";

            sqlx::query(sql)
                .bind(blob_id.as_str())
                .bind(&json_str)
                .execute(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(blob_id)
        })
    }

    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<ValueRef, StateError>> + Send + '_>> {
        let blob_id = blob_id.clone();
        Box::pin(async move {
            let sql = "SELECT data FROM blobs WHERE id = ?";

            let row = sqlx::query(sql)
                .bind(blob_id.as_str())
                .fetch_optional(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            let row = row.ok_or_else(|| {
                error_stack::report!(StateError::BlobNotFound {
                    blob_id: blob_id.as_str().to_string(),
                })
            })?;

            let json_str: String = row.get("data");
            let value: serde_json::Value =
                serde_json::from_str(&json_str).change_context(StateError::Serialization)?;

            Ok(ValueRef::new(value))
        })
    }

    fn record_step_result(
        &self,
        execution_id: Uuid,
        step_result: StepResult<'_>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>> {
        // Convert to owned version to move into async block
        let step_result = step_result.to_owned();
        Box::pin(async move {
            // Ensure execution exists
            self.ensure_execution_exists(execution_id)
                .await
                .change_context(StateError::Internal)?;

            // Serialize the FlowResult
            let result_json = serde_json::to_string(step_result.result())
                .change_context(StateError::Serialization)
                .change_context(StateError::Internal)?;

            // Insert or replace step result
            let sql = "INSERT OR REPLACE INTO step_results (execution_id, step_index, step_id, result) VALUES (?, ?, ?, ?)";

            sqlx::query(sql)
                .bind(execution_id.to_string())
                .bind(step_result.step_idx() as i64)
                .bind(step_result.step_id())
                .bind(&result_json)
                .execute(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        })
    }

    fn get_step_result_by_index(
        &self,
        execution_id: Uuid,
        step_idx: usize,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<FlowResult, StateError>> + Send + '_>>
    {
        Box::pin(async move {
            let sql = "SELECT result FROM step_results WHERE execution_id = ? AND step_index = ?";

            let row = sqlx::query(sql)
                .bind(execution_id.to_string())
                .bind(step_idx as i64)
                .fetch_optional(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            let row = row.ok_or_else(|| {
                error_stack::report!(StateError::StepResultNotFoundByIndex {
                    execution_id: execution_id.to_string(),
                    step_idx,
                })
            })?;

            let result_json: String = row.get("result");
            let flow_result: FlowResult =
                serde_json::from_str(&result_json).change_context(StateError::Serialization)?;

            Ok(flow_result)
        })
    }

    fn get_step_result_by_id(
        &self,
        execution_id: Uuid,
        step_id: &str,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<FlowResult, StateError>> + Send + '_>>
    {
        let step_id = step_id.to_string();
        Box::pin(async move {
            let sql = "SELECT result FROM step_results WHERE execution_id = ? AND step_id = ?";

            let row = sqlx::query(sql)
                .bind(execution_id.to_string())
                .bind(&step_id)
                .fetch_optional(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            let row = row.ok_or_else(|| {
                error_stack::report!(StateError::StepResultNotFoundById {
                    execution_id,
                    step_id: step_id.clone(),
                })
            })?;

            let result_json: String = row.get("result");
            let flow_result: FlowResult =
                serde_json::from_str(&result_json).change_context(StateError::Serialization)?;

            Ok(flow_result)
        })
    }

    fn list_step_results(
        &self,
        execution_id: Uuid,
    ) -> Pin<
        Box<
            dyn Future<Output = error_stack::Result<Vec<StepResult<'static>>, StateError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let sql = "SELECT step_index, step_id, result FROM step_results WHERE execution_id = ? ORDER BY step_index";

            let rows = sqlx::query(sql)
                .bind(execution_id.to_string())
                .fetch_all(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            let mut results = Vec::new();
            for row in rows {
                let step_index: i64 = row.get("step_index");
                let step_id: Option<String> = row.get("step_id");
                let result_json: String = row.get("result");

                let flow_result: FlowResult =
                    serde_json::from_str(&result_json).change_context(StateError::Serialization)?;

                let step_result = StepResult::new(
                    step_index as usize,
                    step_id.unwrap_or_else(|| format!("step_{}", step_index)),
                    flow_result,
                );

                results.push(step_result);
            }

            Ok(results)
        })
    }
}
