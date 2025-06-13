use std::sync::Arc;

use error_stack::{Result, ResultExt as _};
use futures::future::{BoxFuture, FutureExt as _};
use sqlx::{Row as _, SqlitePool, sqlite::SqlitePoolOptions};
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowResult,
    blob::BlobId,
    workflow::{Component, Flow, ValueRef},
};
use stepflow_state::{
    CreateExecutionParams, DebugSessionData, Endpoint, ExecutionDetails, ExecutionFilters,
    ExecutionStepDetails, ExecutionSummary, ExecutionWithBlobs, StateError, StateStore, StepInfo,
    StepResult,
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
    fn put_blob(&self, data: ValueRef) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        async move {
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
        }
        .boxed()
    }

    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<ValueRef, StateError>> {
        let blob_id = blob_id.clone();
        async move {
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
        }
        .boxed()
    }

    fn record_step_result(
        &self,
        execution_id: Uuid,
        step_result: StepResult<'_>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        // Convert to owned version to move into async block
        let step_result = step_result.to_owned();
        async move {
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
        }.boxed()
    }

    fn get_step_result_by_index(
        &self,
        execution_id: Uuid,
        step_idx: usize,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>> {
        async move {
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
        }
        .boxed()
    }

    fn get_step_result_by_id(
        &self,
        execution_id: Uuid,
        step_id: &str,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>> {
        let step_id = step_id.to_string();
        async move {
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
        }
        .boxed()
    }

    fn list_step_results(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepResult<'static>>, StateError>> {
        async move {
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
        }.boxed()
    }

    // Workflow Management Methods

    fn store_workflow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<FlowHash, StateError>> {
        let pool = self.pool.clone();
        let workflow_json = serde_json::to_string(workflow.as_ref());
        let workflow_hash = Flow::hash(workflow.as_ref());

        async move {
            let workflow_json = workflow_json.change_context(StateError::Serialization)?;

            // Store the workflow (INSERT OR IGNORE for deduplication)
            let sql = "INSERT OR IGNORE INTO workflows (hash, content) VALUES (?, ?)";

            sqlx::query(sql)
                .bind(workflow_hash.to_string())
                .bind(&workflow_json)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(workflow_hash)
        }
        .boxed()
    }

    fn get_workflow(
        &self,
        workflow_hash: &str,
    ) -> BoxFuture<'_, error_stack::Result<Arc<Flow>, StateError>> {
        let pool = self.pool.clone();
        let hash = workflow_hash.to_string();

        async move {
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

            Ok(Arc::new(workflow))
        }
        .boxed()
    }

    fn create_endpoint(
        &self,
        name: &str,
        label: Option<&str>,
        workflow_hash: &str,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());
        let workflow_hash = workflow_hash.to_string();

        async move {
            let sql = "INSERT OR REPLACE INTO endpoints (name, label, workflow_hash, updated_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP)";

            sqlx::query(sql)
                .bind(&name)
                .bind(&label)
                .bind(&workflow_hash)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }.boxed()
    }

    fn get_endpoint(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<Endpoint, StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());

        async move {
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
        }.boxed()
    }

    fn list_endpoints(
        &self,
        name_filter: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<Vec<Endpoint>, StateError>> {
        let pool = self.pool.clone();
        let name_filter = name_filter.map(|s| s.to_string());

        async move {
            let (sql, bind_name) = if let Some(ref name) = name_filter {
                (
                    "SELECT name, label, workflow_hash, created_at, updated_at FROM endpoints WHERE name = ? ORDER BY created_at DESC",
                    Some(name),
                )
            } else {
                (
                    "SELECT name, label, workflow_hash, created_at, updated_at FROM endpoints ORDER BY created_at DESC",
                    None,
                )
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
        }.boxed()
    }

    fn delete_endpoint(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());

        async move {
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
        }
        .boxed()
    }

    fn create_execution(
        &self,
        execution_id: Uuid,
        endpoint_name: Option<&str>,
        endpoint_label: Option<&str>,
        workflow_hash: Option<&str>,
        debug_mode: bool,
        input_blob_id: Option<&BlobId>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let endpoint_name = endpoint_name.map(|s| s.to_string());
        let endpoint_label = endpoint_label.map(|s| s.to_string());
        let workflow_hash = workflow_hash.map(|s| s.to_string());
        let input_blob_id = input_blob_id.map(|id| id.as_str().to_string());

        async move {
            let sql = "INSERT INTO executions (id, endpoint_name, endpoint_label, workflow_hash, status, debug_mode, input_blob_id) VALUES (?, ?, ?, ?, 'running', ?, ?)";

            sqlx::query(sql)
                .bind(execution_id.to_string())
                .bind(endpoint_name)
                .bind(endpoint_label)
                .bind(workflow_hash)
                .bind(debug_mode)
                .bind(input_blob_id)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }.boxed()
    }

    fn update_execution_status(
        &self,
        execution_id: Uuid,
        status: ExecutionStatus,
        result_blob_id: Option<&BlobId>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let result_blob_id = result_blob_id.map(|id| id.as_str().to_string());

        async move {
            let sql = match status {
                ExecutionStatus::Completed | ExecutionStatus::Failed => {
                    "UPDATE executions SET status = ?, result_blob_id = ?, completed_at = CURRENT_TIMESTAMP WHERE id = ?"
                }
                _ => "UPDATE executions SET status = ?, result_blob_id = ? WHERE id = ?",
            };

            sqlx::query(sql)
                .bind(status.as_str())
                .bind(&result_blob_id)
                .bind(execution_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }.boxed()
    }

    fn get_execution(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<ExecutionDetails, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT id, endpoint_name, endpoint_label, workflow_hash, status, debug_mode, input_blob_id, result_blob_id, created_at, completed_at FROM executions WHERE id = ?";

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
                _ => {
                    tracing::warn!("Unrecognized execution status: {status_str}");
                    ExecutionStatus::Running
                }
            };

            let endpoint_name = row.get::<Option<String>, _>("endpoint_name");
            let endpoint_label = row.get::<Option<String>, _>("endpoint_label");

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
        }.boxed()
    }

    fn list_executions(
        &self,
        filters: &ExecutionFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<ExecutionSummary>, StateError>> {
        let pool = self.pool.clone();
        let filters = filters.clone();

        async move {
            let mut sql = "SELECT id, endpoint_name, endpoint_label, workflow_hash, status, debug_mode, created_at, completed_at FROM executions".to_string();
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

                let endpoint_name = row.get::<Option<String>, _>("endpoint_name");
                let endpoint_label = row.get::<Option<String>, _>("endpoint_label");

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
        }.boxed()
    }

    // Compound Query Methods (for server optimization)

    fn get_endpoint_with_workflow(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<(Endpoint, Arc<Flow>), StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());

        async move {
            let sql = r#"
                SELECT e.name, e.label, e.workflow_hash, e.created_at, e.updated_at, w.content
                FROM endpoints e
                JOIN workflows w ON e.workflow_hash = w.hash
                WHERE e.name = ? AND e.label IS ?
            "#;

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

            let content: String = row.get("content");
            let workflow: Flow =
                serde_json::from_str(&content).change_context(StateError::Serialization)?;

            Ok((endpoint, Arc::new(workflow)))
        }
        .boxed()
    }

    fn get_execution_with_workflow(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(ExecutionDetails, Option<Arc<Flow>>), StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = r#"
                SELECT e.id, e.endpoint_name, e.endpoint_label, e.workflow_hash, e.status, e.debug_mode,
                       e.input_blob_id, e.result_blob_id, e.created_at, e.completed_at, w.content
                FROM executions e
                LEFT JOIN workflows w ON e.workflow_hash = w.hash
                WHERE e.id = ?
            "#;

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

            let endpoint_name = row.get::<Option<String>, _>("endpoint_name");
            let endpoint_label = row.get::<Option<String>, _>("endpoint_label");

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

            let workflow = if let Some(content) = row.get::<Option<String>, _>("content") {
                let flow: Flow =
                    serde_json::from_str(&content).change_context(StateError::Serialization)?;
                Some(Arc::new(flow))
            } else {
                None
            };

            Ok((details, workflow))
        }.boxed()
    }

    fn get_execution_with_blobs(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<ExecutionWithBlobs, StateError>> {
        let _pool = self.pool.clone();

        async move {
            // First get the execution details
            let execution = self.get_execution(execution_id).await?;

            // Then fetch the input and result blobs if they exist
            let input = if let Some(ref input_blob_id) = execution.input_blob_id {
                Some(self.get_blob(input_blob_id).await?)
            } else {
                None
            };

            let result = if let Some(ref result_blob_id) = execution.result_blob_id {
                Some(self.get_blob(result_blob_id).await?)
            } else {
                None
            };

            Ok(ExecutionWithBlobs {
                execution,
                input,
                result,
            })
        }
        .boxed()
    }

    fn get_execution_step_details(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<ExecutionStepDetails, StateError>> {
        let _pool = self.pool.clone();

        async move {
            // Get execution with workflow
            let (execution, workflow) = self.get_execution_with_workflow(execution_id).await?;

            // Get step results
            let step_results = self.list_step_results(execution_id).await?;

            // Get input blob if it exists
            let input = if let Some(ref input_blob_id) = execution.input_blob_id {
                Some(self.get_blob(input_blob_id).await?)
            } else {
                None
            };

            Ok(ExecutionStepDetails {
                execution,
                workflow,
                step_results,
                input,
            })
        }
        .boxed()
    }

    fn get_debug_session_data(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<DebugSessionData, StateError>> {
        async move {
            // Get execution with workflow
            let (execution, workflow) = self.get_execution_with_workflow(execution_id).await?;

            let workflow = workflow.ok_or_else(|| {
                error_stack::report!(StateError::WorkflowNotFound {
                    workflow_hash: execution.workflow_hash.clone().unwrap_or_default()
                })
            })?;

            // Get step results
            let step_results = self.list_step_results(execution_id).await?;

            // Get input blob (required for debug session)
            let input = if let Some(ref input_blob_id) = execution.input_blob_id {
                self.get_blob(input_blob_id).await?
            } else {
                return Err(error_stack::report!(StateError::BlobNotFound {
                    blob_id: "input_blob_id is None".to_string()
                }));
            };

            Ok(DebugSessionData {
                execution,
                workflow,
                input,
                step_results,
            })
        }
        .boxed()
    }

    // Atomic Operations (for consistency)

    fn create_endpoint_with_workflow(
        &self,
        name: &str,
        label: Option<&str>,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<(String, Endpoint), StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());

        async move {
            let workflow_json = serde_json::to_string(workflow.as_ref())
                .change_context(StateError::Serialization)?;

            // Start a transaction for atomicity
            let mut tx = pool.begin().await.change_context(StateError::Internal)?;

            // Generate SHA-256 hash of the workflow content
            use sha2::{Digest as _, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(workflow_json.as_bytes());
            let workflow_hash = format!("{:x}", hasher.finalize());

            // Store the workflow (INSERT OR IGNORE for deduplication)
            let workflow_sql = "INSERT OR IGNORE INTO workflows (hash, content) VALUES (?, ?)";
            sqlx::query(workflow_sql)
                .bind(&workflow_hash)
                .bind(&workflow_json)
                .execute(&mut *tx)
                .await
                .change_context(StateError::Internal)?;

            // Create the endpoint
            let endpoint_sql = "INSERT OR REPLACE INTO endpoints (name, label, workflow_hash, updated_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP)";
            sqlx::query(endpoint_sql)
                .bind(&name)
                .bind(&label)
                .bind(&workflow_hash)
                .execute(&mut *tx)
                .await
                .change_context(StateError::Internal)?;

            // Commit the transaction
            tx.commit().await.change_context(StateError::Internal)?;

            // Get the created endpoint
            let endpoint = self.get_endpoint(&name, label.as_deref()).await?;

            Ok((workflow_hash, endpoint))
        }.boxed()
    }

    fn create_execution_with_input<'a>(
        &'a self,
        execution_id: Uuid,
        params: CreateExecutionParams<'a>,
        input: Option<ValueRef>,
    ) -> BoxFuture<'a, error_stack::Result<ExecutionDetails, StateError>> {
        let pool = self.pool.clone();

        async move {
            // Start a transaction for atomicity
            let mut tx = pool.begin().await.change_context(StateError::Internal)?;

            // Store input as blob if provided
            let input_blob_id = if let Some(input_data) = input {
                let blob_id =
                    BlobId::from_content(&input_data).change_context(StateError::Internal)?;
                let json_str = serde_json::to_string(input_data.as_ref())
                    .change_context(StateError::Serialization)?;

                let blob_sql = "INSERT OR IGNORE INTO blobs (id, data) VALUES (?, ?)";
                sqlx::query(blob_sql)
                    .bind(blob_id.as_str())
                    .bind(&json_str)
                    .execute(&mut *tx)
                    .await
                    .change_context(StateError::Internal)?;

                Some(blob_id)
            } else {
                None
            };

            // Create the execution
            let execution_sql = "INSERT INTO executions (id, endpoint_name, endpoint_label, workflow_hash, status, debug_mode, input_blob_id) VALUES (?, ?, ?, ?, 'running', ?, ?)";
            sqlx::query(execution_sql)
                .bind(execution_id.to_string())
                .bind(params.endpoint_name)
                .bind(params.endpoint_label)
                .bind(params.workflow_hash)
                .bind(params.debug_mode)
                .bind(input_blob_id.as_ref().map(|id| id.as_str()))
                .execute(&mut *tx)
                .await
                .change_context(StateError::Internal)?;

            // Commit the transaction
            tx.commit().await.change_context(StateError::Internal)?;

            // Get the created execution
            let execution = self.get_execution(execution_id).await?;

            Ok(execution)
        }.boxed()
    }

    // Optimized Query Methods (for value resolution)

    fn try_get_step_result_by_id(
        &self,
        execution_id: Uuid,
        step_id: &str,
    ) -> BoxFuture<'_, error_stack::Result<Option<FlowResult>, StateError>> {
        let step_id = step_id.to_string();
        async move {
            let sql = "SELECT result FROM step_results WHERE execution_id = ? AND step_id = ?";

            let row = sqlx::query(sql)
                .bind(execution_id.to_string())
                .bind(&step_id)
                .fetch_optional(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            if let Some(row) = row {
                let result_json: String = row.get("result");
                let flow_result: FlowResult =
                    serde_json::from_str(&result_json).change_context(StateError::Serialization)?;
                Ok(Some(flow_result))
            } else {
                Ok(None)
            }
        }
        .boxed()
    }

    fn get_step_results_by_indices(
        &self,
        execution_id: Uuid,
        indices: &[usize],
    ) -> BoxFuture<'_, error_stack::Result<Vec<Option<FlowResult>>, StateError>> {
        let indices = indices.to_vec();
        async move {
            if indices.is_empty() {
                return Ok(Vec::new());
            }

            // Create SQL query with IN clause for batch retrieval
            let placeholders = indices.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT step_index, result FROM step_results WHERE execution_id = ? AND step_index IN ({})",
                placeholders
            );

            let mut query = sqlx::query(&sql).bind(execution_id.to_string());
            for index in &indices {
                query = query.bind(*index as i64);
            }

            let rows = query
                .fetch_all(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            // Create a map from step_index to result
            let mut results_map = std::collections::HashMap::new();
            for row in rows {
                let step_index: i64 = row.get("step_index");
                let result_json: String = row.get("result");
                let flow_result: FlowResult =
                    serde_json::from_str(&result_json).change_context(StateError::Serialization)?;
                results_map.insert(step_index as usize, flow_result);
            }

            // Build result vector in the order of requested indices
            let mut results = Vec::with_capacity(indices.len());
            for index in indices {
                results.push(results_map.remove(&index));
            }

            Ok(results)
        }.boxed()
    }

    // Step Status Management

    fn initialize_step_info(
        &self,
        execution_id: Uuid,
        steps: &[StepInfo],
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let steps = steps.to_vec();
        let pool = self.pool.clone();

        async move {
            // Delete existing step info for this execution
            let delete_sql = "DELETE FROM step_info WHERE execution_id = ?";
            sqlx::query(delete_sql)
                .bind(execution_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            // Insert new step info
            if !steps.is_empty() {
                let insert_sql = "INSERT INTO step_info (execution_id, step_index, step_id, component, status) VALUES (?, ?, ?, ?, ?)";

                for step in steps {
                    let status_str = match step.status {
                        stepflow_core::status::StepStatus::Blocked => "blocked",
                        stepflow_core::status::StepStatus::Runnable => "runnable",
                        stepflow_core::status::StepStatus::Running => "running",
                        stepflow_core::status::StepStatus::Completed => "completed",
                        stepflow_core::status::StepStatus::Failed => "failed",
                        stepflow_core::status::StepStatus::Skipped => "skipped",
                    };

                    sqlx::query(insert_sql)
                        .bind(execution_id.to_string())
                        .bind(step.step_index as i64)
                        .bind(&step.step_id)
                        .bind(step.component.to_string())
                        .bind(status_str)
                        .execute(&pool)
                        .await
                        .change_context(StateError::Internal)?;
                }
            }

            Ok(())
        }.boxed()
    }

    fn update_step_status(
        &self,
        execution_id: Uuid,
        step_index: usize,
        status: stepflow_core::status::StepStatus,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            let status_str = match status {
                stepflow_core::status::StepStatus::Blocked => "blocked",
                stepflow_core::status::StepStatus::Runnable => "runnable",
                stepflow_core::status::StepStatus::Running => "running",
                stepflow_core::status::StepStatus::Completed => "completed",
                stepflow_core::status::StepStatus::Failed => "failed",
                stepflow_core::status::StepStatus::Skipped => "skipped",
            };

            let sql = "UPDATE step_info SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE execution_id = ? AND step_index = ?";

            sqlx::query(sql)
                .bind(status_str)
                .bind(execution_id.to_string())
                .bind(step_index as i64)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }.boxed()
    }

    fn get_step_info_for_execution(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT step_index, step_id, component, status, created_at, updated_at FROM step_info WHERE execution_id = ? ORDER BY step_index";

            let rows = sqlx::query(sql)
                .bind(execution_id.to_string())
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut step_infos = Vec::new();
            for row in rows {
                let status_str: String = row.get("status");
                let status = match status_str.as_str() {
                    "blocked" => stepflow_core::status::StepStatus::Blocked,
                    "runnable" => stepflow_core::status::StepStatus::Runnable,
                    "running" => stepflow_core::status::StepStatus::Running,
                    "completed" => stepflow_core::status::StepStatus::Completed,
                    "failed" => stepflow_core::status::StepStatus::Failed,
                    "skipped" => stepflow_core::status::StepStatus::Skipped,
                    _ => stepflow_core::status::StepStatus::Blocked, // Default fallback
                };

                let component_str: String = row.get("component");
                let component =
                    Component::parse(&component_str).change_context(StateError::Internal)?;

                let step_info = StepInfo {
                    execution_id,
                    step_index: row.get::<i64, _>("step_index") as usize,
                    step_id: row.get("step_id"),
                    component,
                    status,
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

                step_infos.push(step_info);
            }

            Ok(step_infos)
        }.boxed()
    }

    fn get_runnable_steps(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>> {
        let pool = self.pool.clone();

        async move {
            // Simply return steps with 'runnable' status - dependency checking is done by the caller
            let sql = r#"
                SELECT step_index, step_id, component, status, created_at, updated_at
                FROM step_info
                WHERE execution_id = ? AND status = 'runnable'
                ORDER BY step_index
            "#;

            let rows = sqlx::query(sql)
                .bind(execution_id.to_string())
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut runnable_steps = Vec::new();
            for row in rows {
                let component_str: String = row.get("component");
                let component =
                    Component::parse(&component_str).change_context(StateError::Internal)?;

                let step_info = StepInfo {
                    execution_id,
                    step_index: row.get::<i64, _>("step_index") as usize,
                    step_id: row.get("step_id"),
                    component,
                    status: stepflow_core::status::StepStatus::Runnable, // These are now runnable
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

                runnable_steps.push(step_info);
            }

            Ok(runnable_steps)
        }
        .boxed()
    }
}
