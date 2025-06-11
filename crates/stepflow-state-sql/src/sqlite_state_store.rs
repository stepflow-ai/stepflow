use std::future::Future;
use std::pin::Pin;

use error_stack::{Result, ResultExt};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use stepflow_core::{
    blob::BlobId,
    workflow::{Flow, ValueRef},
    FlowResult,
};
use stepflow_state::{
    Endpoint, ExecutionDetails, ExecutionFilters, ExecutionStatus, ExecutionSummary, StateError,
    StateStore, StepResult,
};
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

    // Workflow Management Methods

    fn store_workflow(
        &self,
        workflow: &Flow,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<String, StateError>> + Send + '_>> {
        let pool = self.pool.clone();
        let workflow_json = serde_json::to_string(workflow);

        Box::pin(async move {
            let workflow_json = workflow_json.change_context(StateError::Serialization)?;

            // Generate SHA-256 hash of the workflow content
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(workflow_json.as_bytes());
            let hash = format!("{:x}", hasher.finalize());

            // Store the workflow (INSERT OR IGNORE for deduplication)
            let sql = "INSERT OR IGNORE INTO workflows (hash, content) VALUES (?, ?)";

            sqlx::query(sql)
                .bind(&hash)
                .bind(&workflow_json)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(hash)
        })
    }

    fn get_workflow(
        &self,
        workflow_hash: &str,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<Flow, StateError>> + Send + '_>> {
        let pool = self.pool.clone();
        let hash = workflow_hash.to_string();

        Box::pin(async move {
            let sql = "SELECT content FROM workflows WHERE hash = ?";

            let row = sqlx::query(sql)
                .bind(&hash)
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)?;

            let row = row.ok_or_else(|| {
                error_stack::report!(StateError::WorkflowNotFound {
                    workflow_hash: hash
                })
            })?;

            let content: String = row.get("content");
            let workflow: Flow =
                serde_json::from_str(&content).change_context(StateError::Serialization)?;

            Ok(workflow)
        })
    }

    fn create_endpoint(
        &self,
        name: &str,
        label: Option<&str>,
        workflow_hash: &str,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());
        let workflow_hash = workflow_hash.to_string();

        Box::pin(async move {
            let sql = "INSERT OR REPLACE INTO endpoints (name, label, workflow_hash, updated_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP)";

            sqlx::query(sql)
                .bind(&name)
                .bind(&label)
                .bind(&workflow_hash)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        })
    }

    fn get_endpoint(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<Endpoint, StateError>> + Send + '_>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());

        Box::pin(async move {
            let sql = "SELECT name, label, workflow_hash, created_at, updated_at FROM endpoints WHERE name = ? AND label IS ?";

            let row = sqlx::query(sql)
                .bind(&name)
                .bind(&label)
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)?;

            let identifier = if let Some(ref l) = label {
                format!("{}:{}", name, l)
            } else {
                name.clone()
            };

            let row = row.ok_or_else(|| {
                error_stack::report!(StateError::EndpointNotFound { name: identifier })
            })?;

            let endpoint = Endpoint {
                name: row.get("name"),
                label: row.get("label"),
                workflow_hash: row.get("workflow_hash"),
                created_at: chrono::DateTime::parse_from_rfc3339(
                    &row.get::<String, _>("created_at"),
                )
                .change_context(StateError::Internal)?
                .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(
                    &row.get::<String, _>("updated_at"),
                )
                .change_context(StateError::Internal)?
                .with_timezone(&chrono::Utc),
            };

            Ok(endpoint)
        })
    }

    fn list_endpoints(
        &self,
        name_filter: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<Vec<Endpoint>, StateError>> + Send + '_>>
    {
        let pool = self.pool.clone();
        let name_filter = name_filter.map(|s| s.to_string());

        Box::pin(async move {
            let (sql, bind_name) = if let Some(ref name) = name_filter {
                ("SELECT name, label, workflow_hash, created_at, updated_at FROM endpoints WHERE name = ? ORDER BY created_at DESC", Some(name))
            } else {
                ("SELECT name, label, workflow_hash, created_at, updated_at FROM endpoints ORDER BY created_at DESC", None)
            };

            let mut query = sqlx::query(sql);
            if let Some(name) = bind_name {
                query = query.bind(name);
            }

            let rows = query
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut endpoints = Vec::new();
            for row in rows {
                let endpoint = Endpoint {
                    name: row.get("name"),
                    label: row.get("label"),
                    workflow_hash: row.get("workflow_hash"),
                    created_at: chrono::DateTime::parse_from_rfc3339(
                        &row.get::<String, _>("created_at"),
                    )
                    .change_context(StateError::Internal)?
                    .with_timezone(&chrono::Utc),
                    updated_at: chrono::DateTime::parse_from_rfc3339(
                        &row.get::<String, _>("updated_at"),
                    )
                    .change_context(StateError::Internal)?
                    .with_timezone(&chrono::Utc),
                };

                endpoints.push(endpoint);
            }

            Ok(endpoints)
        })
    }

    fn delete_endpoint(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());

        Box::pin(async move {
            let sql = if label.as_deref() == Some("*") {
                // Delete all versions of this endpoint
                "DELETE FROM endpoints WHERE name = ?"
            } else {
                // Delete specific version (including default when label is None)
                "DELETE FROM endpoints WHERE name = ? AND label IS ?"
            };

            let mut query = sqlx::query(sql).bind(&name);
            if label.as_deref() != Some("*") {
                query = query.bind(&label);
            }

            query
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        })
    }

    fn create_execution(
        &self,
        execution_id: Uuid,
        endpoint_name: Option<&str>,
        endpoint_label: Option<&str>,
        workflow_hash: Option<&str>,
        debug_mode: bool,
        input_blob_id: Option<&BlobId>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>> {
        let pool = self.pool.clone();
        let endpoint_name = endpoint_name.map(|s| s.to_string());
        let endpoint_label = endpoint_label.map(|s| s.to_string());
        let workflow_hash = workflow_hash.map(|s| s.to_string());
        let input_blob_id = input_blob_id.map(|id| id.as_str().to_string());

        Box::pin(async move {
            // First, we need to add the endpoint_label column to the table through a migration
            // For now, let's store the label as part of the endpoint_name field temporarily
            let combined_endpoint_name =
                if let (Some(name), Some(label)) = (&endpoint_name, &endpoint_label) {
                    Some(format!("{}:{}", name, label))
                } else {
                    endpoint_name
                };

            let sql = "INSERT INTO executions (id, endpoint_name, workflow_hash, status, debug_mode, input_blob_id) VALUES (?, ?, ?, 'running', ?, ?)";

            sqlx::query(sql)
                .bind(execution_id.to_string())
                .bind(combined_endpoint_name)
                .bind(workflow_hash)
                .bind(debug_mode)
                .bind(input_blob_id)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        })
    }

    fn update_execution_status(
        &self,
        execution_id: Uuid,
        status: ExecutionStatus,
        result_blob_id: Option<&BlobId>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>> {
        let pool = self.pool.clone();
        let status_str = match status {
            ExecutionStatus::Running => "running",
            ExecutionStatus::Completed => "completed",
            ExecutionStatus::Failed => "failed",
            ExecutionStatus::Paused => "paused",
        }
        .to_string();
        let result_blob_id = result_blob_id.map(|id| id.as_str().to_string());

        Box::pin(async move {
            let sql = match status {
                ExecutionStatus::Completed | ExecutionStatus::Failed => {
                    "UPDATE executions SET status = ?, result_blob_id = ?, completed_at = CURRENT_TIMESTAMP WHERE id = ?"
                }
                _ => {
                    "UPDATE executions SET status = ?, result_blob_id = ? WHERE id = ?"
                }
            };

            sqlx::query(sql)
                .bind(&status_str)
                .bind(&result_blob_id)
                .bind(execution_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        })
    }

    fn get_execution(
        &self,
        execution_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<ExecutionDetails, StateError>> + Send + '_>>
    {
        let pool = self.pool.clone();

        Box::pin(async move {
            let sql = "SELECT id, endpoint_name, workflow_hash, status, debug_mode, input_blob_id, result_blob_id, created_at, completed_at FROM executions WHERE id = ?";

            let row = sqlx::query(sql)
                .bind(execution_id.to_string())
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)?;

            let row = row.ok_or_else(|| {
                error_stack::report!(StateError::ExecutionNotFound { execution_id })
            })?;

            let status_str: String = row.get("status");
            let status = match status_str.as_str() {
                "running" => ExecutionStatus::Running,
                "completed" => ExecutionStatus::Completed,
                "failed" => ExecutionStatus::Failed,
                "paused" => ExecutionStatus::Paused,
                _ => ExecutionStatus::Running,
            };

            // Parse combined endpoint name (temporary solution)
            let combined_endpoint = row.get::<Option<String>, _>("endpoint_name");
            let (endpoint_name, endpoint_label) = if let Some(ref combined) = combined_endpoint {
                if let Some(colon_pos) = combined.find(':') {
                    let name = combined[..colon_pos].to_string();
                    let label = combined[colon_pos + 1..].to_string();
                    (Some(name), Some(label))
                } else {
                    (Some(combined.clone()), None)
                }
            } else {
                (None, None)
            };

            let details = ExecutionDetails {
                execution_id,
                endpoint_name,
                endpoint_label,
                workflow_hash: row.get("workflow_hash"),
                status,
                debug_mode: row.get("debug_mode"),
                input_blob_id: row
                    .get::<Option<String>, _>("input_blob_id")
                    .and_then(|s| BlobId::new(s).ok()),
                result_blob_id: row
                    .get::<Option<String>, _>("result_blob_id")
                    .and_then(|s| BlobId::new(s).ok()),
                created_at: chrono::DateTime::parse_from_rfc3339(
                    &row.get::<String, _>("created_at"),
                )
                .change_context(StateError::Internal)?
                .with_timezone(&chrono::Utc),
                completed_at: row
                    .get::<Option<String>, _>("completed_at")
                    .map(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .change_context(StateError::Internal)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                    })
                    .transpose()?,
            };

            Ok(details)
        })
    }

    fn list_executions(
        &self,
        filters: &ExecutionFilters,
    ) -> Pin<
        Box<
            dyn Future<Output = error_stack::Result<Vec<ExecutionSummary>, StateError>> + Send + '_,
        >,
    > {
        let pool = self.pool.clone();
        let filters = filters.clone();

        Box::pin(async move {
            let mut sql = "SELECT id, endpoint_name, workflow_hash, status, debug_mode, created_at, completed_at FROM executions".to_string();
            let mut conditions = Vec::new();
            let mut bind_values: Vec<String> = Vec::new();

            if let Some(ref status) = filters.status {
                let status_str = match status {
                    ExecutionStatus::Running => "running",
                    ExecutionStatus::Completed => "completed",
                    ExecutionStatus::Failed => "failed",
                    ExecutionStatus::Paused => "paused",
                };
                conditions.push("status = ?".to_string());
                bind_values.push(status_str.to_string());
            }

            if let Some(ref endpoint_name) = filters.endpoint_name {
                conditions.push("endpoint_name = ?".to_string());
                bind_values.push(endpoint_name.clone());
            }

            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }

            sql.push_str(" ORDER BY created_at DESC");

            if let Some(limit) = filters.limit {
                sql.push_str(" LIMIT ");
                sql.push_str(&limit.to_string());

                if let Some(offset) = filters.offset {
                    sql.push_str(" OFFSET ");
                    sql.push_str(&offset.to_string());
                }
            }

            let mut query = sqlx::query(&sql);
            for value in bind_values {
                query = query.bind(value);
            }

            let rows = query
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut summaries = Vec::new();
            for row in rows {
                let execution_id_str: String = row.get("id");
                let execution_id =
                    Uuid::parse_str(&execution_id_str).change_context(StateError::Internal)?;

                let status_str: String = row.get("status");
                let status = match status_str.as_str() {
                    "running" => ExecutionStatus::Running,
                    "completed" => ExecutionStatus::Completed,
                    "failed" => ExecutionStatus::Failed,
                    "paused" => ExecutionStatus::Paused,
                    _ => ExecutionStatus::Running,
                };

                // Parse combined endpoint name (temporary solution)
                let combined_endpoint = row.get::<Option<String>, _>("endpoint_name");
                let (endpoint_name, endpoint_label) = if let Some(ref combined) = combined_endpoint
                {
                    if let Some(colon_pos) = combined.find(':') {
                        let name = combined[..colon_pos].to_string();
                        let label = combined[colon_pos + 1..].to_string();
                        (Some(name), Some(label))
                    } else {
                        (Some(combined.clone()), None)
                    }
                } else {
                    (None, None)
                };

                let summary = ExecutionSummary {
                    execution_id,
                    endpoint_name,
                    endpoint_label,
                    workflow_hash: row.get("workflow_hash"),
                    status,
                    debug_mode: row.get("debug_mode"),
                    created_at: chrono::DateTime::parse_from_rfc3339(
                        &row.get::<String, _>("created_at"),
                    )
                    .change_context(StateError::Internal)?
                    .with_timezone(&chrono::Utc),
                    completed_at: row
                        .get::<Option<String>, _>("completed_at")
                        .map(|s| {
                            chrono::DateTime::parse_from_rfc3339(&s)
                                .change_context(StateError::Internal)
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                        })
                        .transpose()?,
                };

                summaries.push(summary);
            }

            Ok(summaries)
        })
    }
}
