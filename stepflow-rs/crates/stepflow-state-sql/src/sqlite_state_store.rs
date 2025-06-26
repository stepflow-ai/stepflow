use std::sync::Arc;

use bit_set::BitSet;
use error_stack::{Result, ResultExt as _};
use futures::future::{BoxFuture, FutureExt as _};
use sqlx::{Row as _, SqlitePool, sqlite::SqlitePoolOptions};
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowResult,
    blob::BlobId,
    workflow::{Component, Flow, StepId, ValueRef},
};
use stepflow_state::{
    ExecutionDetails, ExecutionFilters, ExecutionSummary, StateError, StateStore,
    StateWriteOperation, StepInfo, StepResult, WorkflowLabelMetadata, WorkflowWithMetadata,
};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
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

/// SQLite-based StateStore implementation with async write queuing
pub struct SqliteStateStore {
    pool: SqlitePool,
    write_queue: mpsc::UnboundedSender<StateWriteOperation>,
    _background_task: JoinHandle<()>,
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

        // Create write queue and background worker
        let (write_queue, receiver) = mpsc::unbounded_channel();
        let pool_for_worker = pool.clone();
        let background_task = tokio::spawn(Self::process_write_queue(receiver, pool_for_worker));

        Ok(Self {
            pool,
            write_queue,
            _background_task: background_task,
        })
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

    /// Background worker that processes write operations
    async fn process_write_queue(
        mut receiver: mpsc::UnboundedReceiver<StateWriteOperation>,
        pool: SqlitePool,
    ) {
        while let Some(operation) = receiver.recv().await {
            match operation {
                StateWriteOperation::RecordStepResult {
                    execution_id,
                    step_result,
                } => {
                    if let Err(e) =
                        Self::record_step_result_sync(&pool, execution_id, step_result).await
                    {
                        tracing::error!("Failed to record step result: {:?}", e);
                    }
                }
                StateWriteOperation::UpdateStepStatuses {
                    execution_id,
                    status,
                    step_indices,
                } => {
                    if let Err(e) =
                        Self::update_step_statuses_sync(&pool, execution_id, status, step_indices)
                            .await
                    {
                        tracing::error!("Failed to update step statuses: {:?}", e);
                    }
                }
                StateWriteOperation::Flush {
                    execution_id: _,
                    completion_notify,
                } => {
                    // All previous operations are already processed at this point
                    let _ = completion_notify.send(Ok(()));
                }
            }
        }
    }

    /// Synchronous version of create_execution for background worker
    async fn create_execution_sync(
        pool: &SqlitePool,
        execution_id: Uuid,
        workflow_hash: FlowHash,
        workflow_name: Option<String>,
        workflow_label: Option<String>,
        debug_mode: bool,
        input: ValueRef,
    ) -> Result<(), StateError> {
        let input_json =
            serde_json::to_string(input.as_ref()).change_context(StateError::Serialization)?;
        let sql = "INSERT INTO executions (id, workflow_hash, workflow_name, workflow_label, status, debug_mode, input_json) VALUES (?, ?, ?, ?, 'running', ?, ?)";

        sqlx::query(sql)
            .bind(execution_id.to_string())
            .bind(workflow_hash.to_string())
            .bind(workflow_name)
            .bind(workflow_label)
            .bind(debug_mode)
            .bind(&input_json)
            .execute(pool)
            .await
            .change_context(StateError::Internal)?;

        Ok(())
    }

    /// Synchronous version of record_step_result for background worker
    async fn record_step_result_sync(
        pool: &SqlitePool,
        execution_id: Uuid,
        step_result: StepResult,
    ) -> Result<(), StateError> {
        // Ensure execution exists
        Self::ensure_execution_exists_static(pool, execution_id)
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
            .execute(pool)
            .await
            .change_context(StateError::Internal)?;

        Ok(())
    }

    /// Synchronous version of update_step_statuses for background worker
    async fn update_step_statuses_sync(
        pool: &SqlitePool,
        execution_id: Uuid,
        status: StepStatus,
        step_indices: BitSet,
    ) -> Result<(), StateError> {
        if step_indices.is_empty() {
            return Ok(());
        }

        // Begin transaction for batching
        let mut tx = pool.begin().await.change_context(StateError::Internal)?;

        let status_str = match status {
            StepStatus::Blocked => "blocked",
            StepStatus::Runnable => "runnable",
            StepStatus::Running => "running",
            StepStatus::Completed => "completed",
            StepStatus::Failed => "failed",
            StepStatus::Skipped => "skipped",
        };

        let sql = "UPDATE step_info SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE execution_id = ? AND step_index = ?";

        for step_index in step_indices.iter() {
            sqlx::query(sql)
                .bind(status_str)
                .bind(execution_id.to_string())
                .bind(step_index as i64)
                .execute(&mut *tx)
                .await
                .change_context(StateError::Internal)?;
        }

        tx.commit().await.change_context(StateError::Internal)?;
        Ok(())
    }

    /// Static version of ensure_execution_exists for background worker
    async fn ensure_execution_exists_static(
        pool: &SqlitePool,
        execution_id: Uuid,
    ) -> Result<(), StateError> {
        let sql = "SELECT 1 FROM executions WHERE id = ? LIMIT 1";
        let exists = sqlx::query(sql)
            .bind(execution_id.to_string())
            .fetch_optional(pool)
            .await
            .change_context(StateError::Internal)?
            .is_some();

        if !exists {
            return Err(error_stack::report!(StateError::ExecutionNotFound {
                execution_id
            }));
        }

        Ok(())
    }

    /// Create an in-memory SQLite database for testing
    pub async fn in_memory() -> Result<Self, StateError> {
        Self::from_url("sqlite::memory:").await
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
        step_id: StepId,
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
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepResult>, StateError>> {
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
                    step_id.unwrap_or_else(|| format!("step_{step_index}")),
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
        workflow_hash: &FlowHash,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>> {
        let pool = self.pool.clone();
        let hash = workflow_hash.to_string();

        async move {
            let sql = "SELECT content FROM workflows WHERE hash = ?";

            let row = sqlx::query(sql)
                .bind(&hash)
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)?;

            match row {
                Some(row) => {
                    let content: String = row.get("content");
                    let workflow: Flow =
                        serde_json::from_str(&content).change_context(StateError::Serialization)?;
                    Ok(Some(Arc::new(workflow)))
                }
                None => Ok(None),
            }
        }
        .boxed()
    }

    fn get_workflows_by_name(
        &self,
        name: &str,
    ) -> BoxFuture<
        '_,
        error_stack::Result<Vec<(FlowHash, chrono::DateTime<chrono::Utc>)>, StateError>,
    > {
        let pool = self.pool.clone();
        let name = name.to_string();

        async move {
            let sql = "SELECT hash, first_seen FROM workflows w WHERE EXISTS (SELECT 1 FROM workflow_labels l WHERE l.workflow_hash = w.hash AND l.name = ?) OR w.hash IN (SELECT w2.hash FROM workflows w2 WHERE json_extract(w2.content, '$.name') = ?) ORDER BY first_seen DESC";

            let rows = sqlx::query(sql)
                .bind(&name)
                .bind(&name)
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut results = Vec::new();
            for row in rows {
                let workflow_hash = FlowHash::from(row.get::<String, _>("hash").as_str());
                let created_at = chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("first_seen"))
                    .change_context(StateError::Internal)?
                    .with_timezone(&chrono::Utc);
                results.push((workflow_hash, created_at));
            }

            Ok(results)
        }.boxed()
    }

    fn get_named_workflow(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<Option<WorkflowWithMetadata>, StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());

        async move {
            match label {
                Some(label_str) => {
                    // Get workflow by label
                    let sql = "SELECT l.name, l.label, l.workflow_hash, l.created_at, l.updated_at, w.content, w.first_seen FROM workflow_labels l JOIN workflows w ON l.workflow_hash = w.hash WHERE l.name = ? AND l.label = ?";

                    let row = sqlx::query(sql)
                        .bind(&name)
                        .bind(&label_str)
                        .fetch_optional(&pool)
                        .await
                        .change_context(StateError::Internal)?;

                    match row {
                        Some(row) => {
                            let workflow_content: String = row.get("content");
                            let workflow: Flow = serde_json::from_str(&workflow_content)
                                .change_context(StateError::Internal)?;
                            let workflow_hash = FlowHash::from(row.get::<String, _>("workflow_hash").as_str());
                            let label_metadata = WorkflowLabelMetadata {
                                name: row.get("name"),
                                label: row.get("label"),
                                workflow_hash: workflow_hash.clone(),
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
                            let created_at = chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("first_seen"))
                                .change_context(StateError::Internal)?
                                .with_timezone(&chrono::Utc);

                            Ok(Some(WorkflowWithMetadata {
                                workflow: Arc::new(workflow),
                                workflow_hash,
                                created_at,
                                label_info: Some(label_metadata),
                            }))
                        },
                        None => Ok(None),
                    }
                },
                None => {
                    // Get latest workflow by name
                    let sql = "SELECT hash, content, first_seen FROM workflows w WHERE EXISTS (SELECT 1 FROM workflow_labels l WHERE l.workflow_hash = w.hash AND l.name = ?) OR w.hash IN (SELECT w2.hash FROM workflows w2 WHERE json_extract(w2.content, '$.name') = ?) ORDER BY first_seen DESC LIMIT 1";

                    let row = sqlx::query(sql)
                        .bind(&name)
                        .bind(&name)
                        .fetch_optional(&pool)
                        .await
                        .change_context(StateError::Internal)?;

                    match row {
                        Some(row) => {
                            let workflow_content: String = row.get("content");
                            let workflow: Flow = serde_json::from_str(&workflow_content)
                                .change_context(StateError::Internal)?;
                            let workflow_hash = FlowHash::from(row.get::<String, _>("hash").as_str());
                            let created_at = chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("first_seen"))
                                .change_context(StateError::Internal)?
                                .with_timezone(&chrono::Utc);

                            Ok(Some(WorkflowWithMetadata {
                                workflow: Arc::new(workflow),
                                workflow_hash,
                                created_at,
                                label_info: None,
                            }))
                        },
                        None => Ok(None),
                    }
                }
            }
        }.boxed()
    }

    fn create_or_update_label(
        &self,
        name: &str,
        label: &str,
        workflow_hash: FlowHash,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.to_string();

        async move {
            let sql = "INSERT OR REPLACE INTO workflow_labels (name, label, workflow_hash, updated_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP)";

            sqlx::query(sql)
                .bind(&name)
                .bind(&label)
                .bind(workflow_hash.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }.boxed()
    }

    fn list_labels_for_name(
        &self,
        name: &str,
    ) -> BoxFuture<'_, error_stack::Result<Vec<WorkflowLabelMetadata>, StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();

        async move {
            let sql = "SELECT name, label, workflow_hash, created_at, updated_at FROM workflow_labels WHERE name = ? ORDER BY created_at DESC";

            let rows = sqlx::query(sql)
                .bind(&name)
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut labels = Vec::new();
            for row in rows {
                let workflow_label = WorkflowLabelMetadata {
                    name: row.get("name"),
                    label: row.get("label"),
                    workflow_hash: FlowHash::from(row.get::<String, _>("workflow_hash").as_str()),
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

                labels.push(workflow_label);
            }

            Ok(labels)
        }.boxed()
    }

    fn list_workflow_names(&self) -> BoxFuture<'_, error_stack::Result<Vec<String>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT DISTINCT name FROM workflow_labels UNION SELECT DISTINCT json_extract(content, '$.name') as name FROM workflows WHERE json_extract(content, '$.name') IS NOT NULL ORDER BY name";

            let rows = sqlx::query(sql)
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut names = Vec::new();
            for row in rows {
                let name: String = row.get("name");
                names.push(name);
            }

            Ok(names)
        }.boxed()
    }

    fn delete_label(
        &self,
        name: &str,
        label: &str,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.to_string();

        async move {
            let sql = "DELETE FROM workflow_labels WHERE name = ? AND label = ?";

            sqlx::query(sql)
                .bind(&name)
                .bind(&label)
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
        workflow_hash: FlowHash,
        workflow_name: Option<&str>,
        workflow_label: Option<&str>,
        debug_mode: bool,
        input: ValueRef,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        // Execute synchronously to avoid race condition with step results
        let pool = self.pool.clone();
        let workflow_name = workflow_name.map(|s| s.to_string());
        let workflow_label = workflow_label.map(|s| s.to_string());

        async move {
            Self::create_execution_sync(
                &pool,
                execution_id,
                workflow_hash,
                workflow_name,
                workflow_label,
                debug_mode,
                input,
            )
            .await
        }
        .boxed()
    }

    fn update_execution_status(
        &self,
        execution_id: Uuid,
        status: ExecutionStatus,
        result: Option<ValueRef>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            let result_json = if let Some(result) = result {
                Some(serde_json::to_string(result.as_ref()).change_context(StateError::Serialization)?)
            } else {
                None
            };

            let sql = match status {
                ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::Cancelled => {
                    "UPDATE executions SET status = ?, result_json = ?, completed_at = CURRENT_TIMESTAMP WHERE id = ?"
                }
                _ => "UPDATE executions SET status = ?, result_json = ? WHERE id = ?",
            };

            sqlx::query(sql)
                .bind(status.as_str())
                .bind(&result_json)
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
    ) -> BoxFuture<'_, error_stack::Result<Option<ExecutionDetails>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT id, workflow_name, workflow_label, workflow_hash, status, debug_mode, input_json, result_json, created_at, completed_at FROM executions WHERE id = ?";

            let row = sqlx::query(sql)
                .bind(execution_id.to_string())
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)?;

            match row {
                Some(row) => {
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

                    let workflow_name = row.get::<Option<String>, _>("workflow_name");
                    let workflow_label = row.get::<Option<String>, _>("workflow_label");

                    // Parse input JSON
                    let input_json: String = row.get("input_json");
                    let input_value: serde_json::Value = serde_json::from_str(&input_json)
                        .change_context(StateError::Serialization)?;
                    let input = ValueRef::new(input_value);

                    // Parse optional result JSON
                    let result = if let Some(result_json) = row.get::<Option<String>, _>("result_json") {
                        let result_value: serde_json::Value = serde_json::from_str(&result_json)
                            .change_context(StateError::Serialization)?;
                        Some(ValueRef::new(result_value))
                    } else {
                        None
                    };

                    let created_at = chrono::DateTime::parse_from_rfc3339(
                        &row.get::<String, _>("created_at"),
                    )
                    .change_context(StateError::Internal)?
                    .with_timezone(&chrono::Utc);

                    let completed_at = row
                        .get::<Option<String>, _>("completed_at")
                        .map(|s| {
                            chrono::DateTime::parse_from_rfc3339(&s)
                                .change_context(StateError::Internal)
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                        })
                        .transpose()?;

                    let details = ExecutionDetails {
                        summary: ExecutionSummary {
                            execution_id,
                            workflow_name,
                            workflow_label,
                            workflow_hash: FlowHash::from(row.get::<String, _>("workflow_hash").as_str()),
                            status,
                            debug_mode: row.get("debug_mode"),
                            created_at,
                            completed_at,
                        },
                        input,
                        result,
                    };

                    Ok(Some(details))
                },
                None => Ok(None),
            }
        }.boxed()
    }

    fn list_executions(
        &self,
        filters: &ExecutionFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<ExecutionSummary>, StateError>> {
        let pool = self.pool.clone();
        let filters = filters.clone();

        async move {
            let mut sql = "SELECT id, workflow_name, workflow_label, workflow_hash, status, debug_mode, created_at, completed_at FROM executions".to_string();
            let mut conditions = Vec::new();
            let mut bind_values: Vec<String> = Vec::new();

            if let Some(ref status) = filters.status {
                let status_str = match status {
                    ExecutionStatus::Running => "running",
                    ExecutionStatus::Completed => "completed",
                    ExecutionStatus::Failed => "failed",
                    ExecutionStatus::Cancelled => "cancelled",
                    ExecutionStatus::Paused => "paused",
                };
                conditions.push("status = ?".to_string());
                bind_values.push(status_str.to_string());
            }

            if let Some(ref workflow_name) = filters.workflow_name {
                conditions.push("workflow_name = ?".to_string());
                bind_values.push(workflow_name.clone());
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

                let workflow_name = row.get::<Option<String>, _>("workflow_name");
                let workflow_label = row.get::<Option<String>, _>("workflow_label");

                let summary = ExecutionSummary {
                    execution_id,
                    workflow_name,
                    workflow_label,
                    workflow_hash: FlowHash::from(row.get::<String, _>("workflow_hash").as_str()),
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
    ) {
        // Queue the operation for background processing
        let mut step_indices = BitSet::new();
        step_indices.insert(step_index);

        if let Err(e) = self
            .write_queue
            .send(StateWriteOperation::UpdateStepStatuses {
                execution_id,
                status,
                step_indices,
            })
        {
            tracing::error!("Failed to queue step status update: {:?}", e);
        }
    }

    fn flush_pending_writes(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.write_queue.send(StateWriteOperation::Flush {
            execution_id: Some(execution_id),
            completion_notify: tx,
        }) {
            tracing::error!("Failed to queue flush operation: {:?}", e);
            return async move { Err(error_stack::report!(StateError::Internal)) }.boxed();
        }

        async move {
            match rx.await {
                Ok(result) => result.map_err(|e| error_stack::report!(e)),
                Err(_) => Err(error_stack::report!(StateError::Internal)),
            }
        }
        .boxed()
    }

    fn queue_write(&self, operation: StateWriteOperation) -> error_stack::Result<(), StateError> {
        self.write_queue
            .send(operation)
            .map_err(|_| error_stack::report!(StateError::Internal))
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
