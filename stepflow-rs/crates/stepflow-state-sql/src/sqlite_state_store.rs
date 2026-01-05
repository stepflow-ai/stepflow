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

use std::sync::Arc;

use bit_set::BitSet;
use error_stack::{Result, ResultExt as _};
use futures::future::{BoxFuture, FutureExt as _};
use sqlx::{Row as _, SqlitePool, sqlite::SqlitePoolOptions};
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{
    BlobData, BlobId, BlobType, FlowResult,
    workflow::{Component, Flow, ValueRef},
};
use stepflow_state::{
    ItemResult, ItemStatistics, ResultOrder, RunDetails, RunFilters, RunSummary, StateError,
    StateStore, StateWriteOperation, StepInfo, StepResult,
};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::migrations;

/// Parse a datetime string from SQLite.
///
/// SQLite's CURRENT_TIMESTAMP uses format "YYYY-MM-DD HH:MM:SS" but we also
/// want to support RFC3339 format for compatibility.
fn parse_sqlite_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    // Try RFC3339 first (e.g., "2025-12-24T19:26:14Z")
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }

    // Try SQLite's CURRENT_TIMESTAMP format (e.g., "2025-12-24 19:26:14")
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(naive.and_utc());
    }

    None
}

/// Configuration for SqliteStateStore
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
            .change_context(StateError::Connection)
            .attach_printable_lazy(|| format!("Database URL: {}", config.database_url))?;

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
                    run_id,
                    step_result,
                } => {
                    if let Err(e) = Self::record_step_result_sync(&pool, run_id, step_result).await
                    {
                        log::error!("Failed to record step result: {:?}", e);
                    }
                }
                StateWriteOperation::UpdateStepStatuses {
                    run_id,
                    status,
                    step_indices,
                } => {
                    if let Err(e) =
                        Self::update_step_statuses_sync(&pool, run_id, status, step_indices).await
                    {
                        log::error!("Failed to update step statuses: {:?}", e);
                    }
                }
                StateWriteOperation::Flush {
                    run_id: _,
                    completion_notify,
                } => {
                    // All previous operations are already processed at this point
                    let _ = completion_notify.send(Ok(()));
                }
            }
        }
    }

    /// Synchronous version of create_run for background worker
    async fn create_run_sync(
        pool: &SqlitePool,
        params: stepflow_state::CreateRunParams,
    ) -> Result<(), StateError> {
        let overrides_json = if params.overrides.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&params.overrides)
                    .change_context(StateError::Serialization)?,
            )
        };

        // Insert run metadata (items stored separately in run_items)
        // Use INSERT OR IGNORE for idempotent behavior - preserves existing run
        let sql = "INSERT OR IGNORE INTO runs (id, flow_id, flow_name, flow_label, status, overrides_json) VALUES (?, ?, ?, ?, 'running', ?)";

        sqlx::query(sql)
            .bind(params.run_id.to_string())
            .bind(params.flow_id.to_string())
            .bind(&params.workflow_name)
            .bind(&params.workflow_label)
            .bind(&overrides_json)
            .execute(pool)
            .await
            .change_context(StateError::Internal)?;

        // Insert each input as a run_item (also idempotent)
        let item_sql = "INSERT OR IGNORE INTO run_items (run_id, item_index, input_json, status) VALUES (?, ?, ?, 'running')";
        for (idx, input) in params.inputs.iter().enumerate() {
            let input_json =
                serde_json::to_string(input.as_ref()).change_context(StateError::Serialization)?;
            sqlx::query(item_sql)
                .bind(params.run_id.to_string())
                .bind(idx as i64)
                .bind(&input_json)
                .execute(pool)
                .await
                .change_context(StateError::Internal)?;
        }

        Ok(())
    }

    /// Synchronous version of record_step_result for background worker
    async fn record_step_result_sync(
        pool: &SqlitePool,
        run_id: Uuid,
        step_result: StepResult,
    ) -> Result<(), StateError> {
        // Ensure execution exists
        Self::ensure_execution_exists_static(pool, run_id)
            .await
            .change_context(StateError::Internal)?;

        // Serialize the FlowResult
        let result_json = serde_json::to_string(step_result.result())
            .change_context(StateError::Serialization)
            .change_context(StateError::Internal)?;

        // Insert or replace step result
        let sql = "INSERT OR REPLACE INTO step_results (run_id, step_index, step_id, result) VALUES (?, ?, ?, ?)";

        sqlx::query(sql)
            .bind(run_id.to_string())
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
        run_id: Uuid,
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

        let sql = "UPDATE step_info SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE run_id = ? AND step_index = ?";

        for step_index in step_indices.iter() {
            sqlx::query(sql)
                .bind(status_str)
                .bind(run_id.to_string())
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
        run_id: Uuid,
    ) -> Result<(), StateError> {
        let sql = "SELECT 1 FROM runs WHERE id = ? LIMIT 1";
        let exists = sqlx::query(sql)
            .bind(run_id.to_string())
            .fetch_optional(pool)
            .await
            .change_context(StateError::Internal)?
            .is_some();

        if !exists {
            return Err(error_stack::report!(StateError::RunNotFound { run_id }));
        }

        Ok(())
    }

    /// Create an in-memory SQLite database for testing
    pub async fn in_memory() -> Result<Self, StateError> {
        Self::from_url("sqlite::memory:").await
    }
}

impl StateStore for SqliteStateStore {
    fn put_blob(
        &self,
        data: ValueRef,
        blob_type: BlobType,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        async move {
            // Generate content-based ID
            let blob_id = BlobId::from_content(&data).change_context(StateError::Internal)?;

            // Serialize data to JSON string
            let json_str =
                serde_json::to_string(data.as_ref()).change_context(StateError::Serialization)?;
            let type_str = match blob_type {
                BlobType::Flow => "flow",
                BlobType::Data => "data",
            };

            // Store blob with type information
            let sql = "INSERT OR IGNORE INTO blobs (id, data, blob_type) VALUES (?, ?, ?)";

            sqlx::query(sql)
                .bind(blob_id.as_str())
                .bind(&json_str)
                .bind(type_str)
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
    ) -> BoxFuture<'_, error_stack::Result<BlobData, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            let sql = "SELECT data, blob_type FROM blobs WHERE id = ?";

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

            // Get blob type from database
            let type_str: String = row.get("blob_type");
            let blob_type = match type_str.as_str() {
                "flow" => BlobType::Flow,
                "data" => BlobType::Data,
                _ => {
                    // Default to data for unknown types
                    log::warn!("Unknown blob type '{}', defaulting to 'data'", type_str);
                    BlobType::Data
                }
            };

            let blob_data = BlobData::from_value_ref(ValueRef::new(value), blob_type, blob_id)
                .change_context(StateError::Internal)?;
            Ok(blob_data)
        }
        .boxed()
    }

    fn get_step_result(
        &self,
        run_id: Uuid,
        step_idx: usize,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>> {
        async move {
            let sql = "SELECT result FROM step_results WHERE run_id = ? AND step_index = ?";

            let row = sqlx::query(sql)
                .bind(run_id.to_string())
                .bind(step_idx as i64)
                .fetch_optional(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            let row = row.ok_or_else(|| {
                error_stack::report!(StateError::StepResultNotFoundByIndex {
                    run_id: run_id.to_string(),
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

    fn list_step_results(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepResult>, StateError>> {
        async move {
            let sql = "SELECT step_index, step_id, result FROM step_results WHERE run_id = ? ORDER BY step_index";

            let rows = sqlx::query(sql)
                .bind(run_id.to_string())
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

    // Workflow Management Methods using unified blob storage

    fn store_flow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        // Use the unified blob storage
        let workflow_json = serde_json::to_value(workflow.as_ref());
        async move {
            let flow_data = workflow_json.change_context(StateError::Serialization)?;
            let flow_value = ValueRef::new(flow_data);
            self.put_blob(flow_value, BlobType::Flow).await
        }
        .boxed()
    }

    fn get_flow(
        &self,
        flow_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>> {
        let flow_id = flow_id.clone();
        async move {
            match self.get_blob_of_type(&flow_id, BlobType::Flow).await? {
                Some(blob_data) => {
                    let workflow: Flow = serde_json::from_value(blob_data.data().as_ref().clone())
                        .change_context(StateError::Serialization)?;
                    Ok(Some(Arc::new(workflow)))
                }
                None => Ok(None),
            }
        }
        .boxed()
    }

    fn create_run(
        &self,
        params: stepflow_state::CreateRunParams,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        // Execute synchronously to avoid race condition with step results
        let pool = self.pool.clone();

        async move { Self::create_run_sync(&pool, params).await }.boxed()
    }

    fn update_run_status(
        &self,
        run_id: Uuid,
        status: ExecutionStatus,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = match status {
                ExecutionStatus::Completed
                | ExecutionStatus::Failed
                | ExecutionStatus::Cancelled => {
                    "UPDATE runs SET status = ?, completed_at = CURRENT_TIMESTAMP WHERE id = ?"
                }
                _ => "UPDATE runs SET status = ? WHERE id = ?",
            };

            sqlx::query(sql)
                .bind(status.as_str())
                .bind(run_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }
        .boxed()
    }

    fn record_item_result(
        &self,
        run_id: Uuid,
        item_index: usize,
        result: FlowResult,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            // Determine status from result
            let status = match &result {
                FlowResult::Success(_) => "completed",
                FlowResult::Failed(_) => "failed",
            };

            // Serialize result
            let result_json =
                serde_json::to_string(&result).change_context(StateError::Serialization)?;

            // Update the existing run_items row (created by create_run)
            let sql = r#"
                UPDATE run_items
                SET status = ?, result_json = ?, completed_at = CURRENT_TIMESTAMP
                WHERE run_id = ? AND item_index = ?
            "#;

            let result = sqlx::query(sql)
                .bind(status)
                .bind(&result_json)
                .bind(run_id.to_string())
                .bind(item_index as i64)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            if result.rows_affected() == 0 {
                return Err(error_stack::report!(StateError::RunNotFound { run_id }));
            }

            // Check if all items are complete and update runs table if so
            let count_sql = r#"
                SELECT
                    (SELECT COUNT(*) FROM run_items WHERE run_id = ?) as total,
                    (SELECT COUNT(*) FROM run_items WHERE run_id = ? AND status IN ('completed', 'failed')) as done,
                    (SELECT COUNT(*) FROM run_items WHERE run_id = ? AND status = 'failed') as failed
            "#;

            let counts = sqlx::query(count_sql)
                .bind(run_id.to_string())
                .bind(run_id.to_string())
                .bind(run_id.to_string())
                .fetch_one(&pool)
                .await
                .change_context(StateError::Internal)?;

            let total: i64 = counts.get("total");
            let done: i64 = counts.get("done");
            let failed: i64 = counts.get("failed");

            // If all items are done, update the run status
            if done >= total {
                let run_status = if failed > 0 { "failed" } else { "completed" };
                let update_sql =
                    "UPDATE runs SET status = ?, completed_at = CURRENT_TIMESTAMP WHERE id = ?";
                sqlx::query(update_sql)
                    .bind(run_status)
                    .bind(run_id.to_string())
                    .execute(&pool)
                    .await
                    .change_context(StateError::Internal)?;
            }

            Ok(())
        }
        .boxed()
    }

    fn get_run(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<RunDetails>, StateError>> {
        let pool = self.pool.clone();

        async move {
            // First get run metadata
            let sql = "SELECT id, flow_name, flow_label, flow_id, status, created_at, completed_at FROM runs WHERE id = ?";

            let row = sqlx::query(sql)
                .bind(run_id.to_string())
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
                            log::warn!("Unrecognized execution status: {status_str}");
                            ExecutionStatus::Running
                        }
                    };

                    let flow_name = row.get::<Option<String>, _>("flow_name");
                    let flow_label = row.get::<Option<String>, _>("flow_label");
                    let flow_id = BlobId::new(row.get::<String, _>("flow_id")).change_context(StateError::Internal)?;

                    let created_at = parse_sqlite_datetime(&row.get::<String, _>("created_at"))
                        .ok_or_else(|| error_stack::report!(StateError::Internal))?;

                    let completed_at = row
                        .get::<Option<String>, _>("completed_at")
                        .and_then(|s| parse_sqlite_datetime(&s));

                    // Fetch items from run_items table (inputs and status only, not results)
                    let items_sql = "SELECT item_index, input_json, status FROM run_items WHERE run_id = ? ORDER BY item_index";
                    let item_rows = sqlx::query(items_sql)
                        .bind(run_id.to_string())
                        .fetch_all(&pool)
                        .await
                        .change_context(StateError::Internal)?;

                    let mut inputs = Vec::new();
                    let mut running = 0usize;
                    let mut completed = 0usize;
                    let mut failed = 0usize;
                    let mut cancelled = 0usize;

                    for item_row in item_rows {
                        // Parse input
                        let input_json: String = item_row.get("input_json");
                        let input_value: serde_json::Value = serde_json::from_str(&input_json)
                            .change_context(StateError::Serialization)?;
                        inputs.push(ValueRef::new(input_value));

                        // Count item statuses for statistics
                        let item_status: String = item_row.get("status");
                        match item_status.as_str() {
                            "running" => running += 1,
                            "completed" => completed += 1,
                            "failed" => failed += 1,
                            "cancelled" => cancelled += 1,
                            _ => running += 1,
                        }
                    }

                    let items = ItemStatistics {
                        total: inputs.len(),
                        running,
                        completed,
                        failed,
                        cancelled,
                    };

                    let details = RunDetails {
                        summary: RunSummary {
                            run_id,
                            flow_name,
                            flow_label,
                            flow_id,
                            status,
                            items,
                            created_at,
                            completed_at,
                        },
                        inputs,
                        overrides: None, // TODO: Store and retrieve overrides from database
                    };

                    Ok(Some(details))
                },
                None => Ok(None),
            }
        }.boxed()
    }

    fn get_run_overrides(
        &self,
        _run_id: Uuid,
    ) -> BoxFuture<
        '_,
        error_stack::Result<Option<stepflow_core::workflow::WorkflowOverrides>, StateError>,
    > {
        // TODO: Implement override storage/retrieval in SQL database
        async move { Ok(None) }.boxed()
    }

    fn list_runs(
        &self,
        filters: &RunFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RunSummary>, StateError>> {
        let pool = self.pool.clone();
        let filters = filters.clone();

        async move {
            // Use a subquery to get item statistics per run
            let mut sql = r#"
                SELECT
                    r.id, r.flow_name, r.flow_label, r.flow_id, r.status,
                    r.created_at, r.completed_at,
                    COALESCE(i.total, 0) as item_total,
                    COALESCE(i.running, 0) as item_running,
                    COALESCE(i.completed, 0) as item_completed,
                    COALESCE(i.failed, 0) as item_failed,
                    COALESCE(i.cancelled, 0) as item_cancelled
                FROM runs r
                LEFT JOIN (
                    SELECT
                        run_id,
                        COUNT(*) as total,
                        SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END) as running,
                        SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as completed,
                        SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed,
                        SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END) as cancelled
                    FROM run_items
                    GROUP BY run_id
                ) i ON r.id = i.run_id
            "#
            .to_string();
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
                conditions.push("r.status = ?".to_string());
                bind_values.push(status_str.to_string());
            }

            if let Some(ref flow_name) = filters.flow_name {
                conditions.push("r.flow_name = ?".to_string());
                bind_values.push(flow_name.clone());
            }

            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }

            sql.push_str(" ORDER BY r.created_at DESC");

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
                let run_id_str: String = row.get("id");
                let run_id = Uuid::parse_str(&run_id_str).change_context(StateError::Internal)?;

                let status_str: String = row.get("status");
                let status = match status_str.as_str() {
                    "running" => ExecutionStatus::Running,
                    "completed" => ExecutionStatus::Completed,
                    "failed" => ExecutionStatus::Failed,
                    "paused" => ExecutionStatus::Paused,
                    _ => ExecutionStatus::Running,
                };

                let flow_name = row.get::<Option<String>, _>("flow_name");
                let flow_label = row.get::<Option<String>, _>("flow_label");
                let flow_id = BlobId::new(row.get::<String, _>("flow_id"))
                    .change_context(StateError::Internal)?;

                let items = ItemStatistics {
                    total: row.get::<i64, _>("item_total") as usize,
                    running: row.get::<i64, _>("item_running") as usize,
                    completed: row.get::<i64, _>("item_completed") as usize,
                    failed: row.get::<i64, _>("item_failed") as usize,
                    cancelled: row.get::<i64, _>("item_cancelled") as usize,
                };

                let created_at = parse_sqlite_datetime(&row.get::<String, _>("created_at"))
                    .ok_or_else(|| error_stack::report!(StateError::Internal))?;

                let completed_at = row
                    .get::<Option<String>, _>("completed_at")
                    .and_then(|s| parse_sqlite_datetime(&s));

                let summary = RunSummary {
                    run_id,
                    flow_name,
                    flow_label,
                    flow_id,
                    status,
                    items,
                    created_at,
                    completed_at,
                };

                summaries.push(summary);
            }

            Ok(summaries)
        }
        .boxed()
    }

    // Step Status Management

    fn initialize_run_steps(
        &self,
        run_id: Uuid,
        steps: &[StepInfo],
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let steps = steps.to_vec();
        let pool = self.pool.clone();

        async move {
            // Delete existing step info for this execution
            let delete_sql = "DELETE FROM step_info WHERE run_id = ?";
            sqlx::query(delete_sql)
                .bind(run_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            // Insert new step info
            if !steps.is_empty() {
                let insert_sql = "INSERT INTO step_info (run_id, step_index, step_id, component, status) VALUES (?, ?, ?, ?, ?)";

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
                        .bind(run_id.to_string())
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
        run_id: Uuid,
        step_index: usize,
        status: stepflow_core::status::StepStatus,
    ) {
        // Queue the operation for background processing
        let mut step_indices = BitSet::new();
        step_indices.insert(step_index);

        if let Err(e) = self
            .write_queue
            .send(StateWriteOperation::UpdateStepStatuses {
                run_id,
                status,
                step_indices,
            })
        {
            log::error!("Failed to queue step status update: {:?}", e);
        }
    }

    fn flush_pending_writes(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let (tx, rx) = oneshot::channel();

        if let Err(e) = self.write_queue.send(StateWriteOperation::Flush {
            run_id: Some(run_id),
            completion_notify: tx,
        }) {
            log::error!("Failed to queue flush operation: {:?}", e);
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

    fn get_step_info_for_run(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT step_index, step_id, component, status, created_at, updated_at FROM step_info WHERE run_id = ? ORDER BY step_index";

            let rows = sqlx::query(sql)
                .bind(run_id.to_string())
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
                let component = Component::from_string(&component_str);

                let step_info = StepInfo {
                    run_id,
                    step_index: row.get::<i64, _>("step_index") as usize,
                    step_id: row.get("step_id"),
                    component,
                    status,
                    created_at: parse_sqlite_datetime(&row.get::<String, _>("created_at"))
                        .ok_or_else(|| error_stack::report!(StateError::Internal))?,
                    updated_at: parse_sqlite_datetime(&row.get::<String, _>("updated_at"))
                        .ok_or_else(|| error_stack::report!(StateError::Internal))?,
                };

                step_infos.push(step_info);
            }

            Ok(step_infos)
        }.boxed()
    }

    fn get_item_results(
        &self,
        run_id: Uuid,
        order: ResultOrder,
    ) -> BoxFuture<'_, error_stack::Result<Vec<ItemResult>, StateError>> {
        let pool = self.pool.clone();
        let run_id_str = run_id.to_string();

        async move {
            // Choose ordering based on order parameter
            let order_clause = match order {
                ResultOrder::ByIndex => "ORDER BY item_index",
                ResultOrder::ByCompletion => {
                    // Items with completed_at first (sorted by time), then incomplete items by index
                    "ORDER BY CASE WHEN completed_at IS NULL THEN 1 ELSE 0 END, completed_at, item_index"
                }
            };

            let sql = format!(
                "SELECT item_index, status, result_json, completed_at FROM run_items WHERE run_id = ? {}",
                order_clause
            );

            let rows = sqlx::query(&sql)
                .bind(&run_id_str)
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut items = Vec::new();
            for row in rows {
                let item_index: i64 = row.get("item_index");
                let status_str: String = row.get("status");
                let status = match status_str.as_str() {
                    "completed" => ExecutionStatus::Completed,
                    "failed" => ExecutionStatus::Failed,
                    "cancelled" => ExecutionStatus::Cancelled,
                    _ => ExecutionStatus::Running,
                };

                let result = if let Some(result_json) = row.get::<Option<String>, _>("result_json")
                {
                    let flow_result: FlowResult = serde_json::from_str(&result_json)
                        .change_context(StateError::Serialization)?;
                    Some(flow_result)
                } else {
                    None
                };

                let completed_at = row
                    .get::<Option<String>, _>("completed_at")
                    .and_then(|s| parse_sqlite_datetime(&s));

                items.push(ItemResult {
                    item_index: item_index as usize,
                    status,
                    result,
                    completed_at,
                });
            }

            Ok(items)
        }
        .boxed()
    }
}
