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

use std::collections::HashSet;

use error_stack::{Result, ResultExt as _};
use futures::future::{BoxFuture, FutureExt as _};
use sqlx::{Row as _, SqlitePool, sqlite::SqlitePoolOptions};
use stepflow_core::status::ExecutionStatus;
use stepflow_core::{BlobData, BlobId, BlobType, FlowResult, workflow::ValueRef};
use stepflow_dtos::{
    ItemDetails, ItemResult, ItemStatistics, ResultOrder, RunDetails, RunFilters, RunSummary,
};
use stepflow_state::{
    BlobStore, ExecutionJournal, JournalEntry, MetadataStore, RootJournalInfo,
    RunCompletionNotifier, SequenceNumber, StateError,
};
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
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
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

/// SQLite-based MetadataStore, BlobStore, and ExecutionJournal implementation.
pub struct SqliteStateStore {
    pool: SqlitePool,
    /// Notifier for run completion events
    completion_notifier: RunCompletionNotifier,
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

        Ok(Self {
            pool,
            completion_notifier: RunCompletionNotifier::new(),
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

    /// Internal helper for creating a run in the database
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
        let sql = "INSERT OR IGNORE INTO runs (id, flow_id, flow_name, status, overrides_json, root_run_id, parent_run_id, orchestrator_id) VALUES (?, ?, ?, 'running', ?, ?, ?, ?)";

        sqlx::query(sql)
            .bind(params.run_id.to_string())
            .bind(params.flow_id.to_string())
            .bind(&params.workflow_name)
            .bind(&overrides_json)
            .bind(params.root_run_id.to_string())
            .bind(params.parent_run_id.map(|id| id.to_string()))
            .bind(&params.orchestrator_id)
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

    /// Create an in-memory SQLite database for testing
    pub async fn in_memory() -> Result<Self, StateError> {
        Self::from_url("sqlite::memory:").await
    }
}

impl BlobStore for SqliteStateStore {
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

    fn get_blob_opt(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<BlobData>, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            let sql = "SELECT data, blob_type FROM blobs WHERE id = ?";

            let row = sqlx::query(sql)
                .bind(blob_id.as_str())
                .fetch_optional(&self.pool)
                .await
                .change_context(StateError::Internal)?;

            let Some(row) = row else {
                return Ok(None);
            };

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
            Ok(Some(blob_data))
        }
        .boxed()
    }

    // Note: store_flow and get_flow use default implementations from the trait
}

impl MetadataStore for SqliteStateStore {
    fn create_run(
        &self,
        params: stepflow_state::CreateRunParams,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        async move { Self::create_run_sync(&pool, params).await }.boxed()
    }

    fn get_run(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<RunDetails>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT id, flow_name, flow_id, status, created_at, completed_at, root_run_id, parent_run_id, orchestrator_id FROM runs WHERE id = ?";

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
                        "cancelled" => ExecutionStatus::Cancelled,
                        "paused" => ExecutionStatus::Paused,
                        "recoveryFailed" => ExecutionStatus::RecoveryFailed,
                        _ => {
                            log::warn!("Unrecognized execution status: {status_str}");
                            ExecutionStatus::Running
                        }
                    };

                    let flow_name = row.get::<Option<String>, _>("flow_name");
                    let flow_id = BlobId::new(row.get::<String, _>("flow_id")).change_context(StateError::Internal)?;

                    let created_at = parse_sqlite_datetime(&row.get::<String, _>("created_at"))
                        .ok_or_else(|| error_stack::report!(StateError::Internal))?;

                    let completed_at = row
                        .get::<Option<String>, _>("completed_at")
                        .and_then(|s| parse_sqlite_datetime(&s));

                    let items_sql = "SELECT item_index, input_json, status, step_statuses_json, completed_at FROM run_items WHERE run_id = ? ORDER BY item_index";
                    let item_rows = sqlx::query(items_sql)
                        .bind(run_id.to_string())
                        .fetch_all(&pool)
                        .await
                        .change_context(StateError::Internal)?;

                    let mut item_details = Vec::new();
                    let mut running = 0usize;
                    let mut completed = 0usize;
                    let mut failed = 0usize;
                    let mut cancelled = 0usize;

                    for item_row in item_rows {
                        let item_index: i64 = item_row.get("item_index");
                        let input_json: String = item_row.get("input_json");
                        let input_value: serde_json::Value = serde_json::from_str(&input_json)
                            .change_context(StateError::Serialization)?;

                        let item_status_str: String = item_row.get("status");
                        let item_status = match item_status_str.as_str() {
                            "running" => {
                                running += 1;
                                ExecutionStatus::Running
                            }
                            "completed" => {
                                completed += 1;
                                ExecutionStatus::Completed
                            }
                            "failed" => {
                                failed += 1;
                                ExecutionStatus::Failed
                            }
                            "cancelled" => {
                                cancelled += 1;
                                ExecutionStatus::Cancelled
                            }
                            _ => {
                                running += 1;
                                ExecutionStatus::Running
                            }
                        };

                        let item_completed_at = item_row
                            .get::<Option<String>, _>("completed_at")
                            .and_then(|s| parse_sqlite_datetime(&s));

                        // Parse step statuses from JSON if available
                        let step_statuses: Vec<stepflow_dtos::StepStatusInfo> = item_row
                            .get::<Option<String>, _>("step_statuses_json")
                            .and_then(|json| serde_json::from_str(&json).ok())
                            .unwrap_or_default();

                        item_details.push(ItemDetails {
                            item_index: item_index as u32,
                            input: ValueRef::new(input_value),
                            status: item_status,
                            steps: step_statuses,
                            completed_at: item_completed_at,
                        });
                    }

                    let items = ItemStatistics {
                        total: item_details.len(),
                        running,
                        completed,
                        failed,
                        cancelled,
                    };

                    let root_run_id_str: Option<String> = row.get("root_run_id");
                    let root_run_id = root_run_id_str
                        .as_ref()
                        .and_then(|s| Uuid::parse_str(s).ok())
                        .unwrap_or(run_id);

                    let parent_run_id_str: Option<String> = row.get("parent_run_id");
                    let parent_run_id = parent_run_id_str
                        .as_ref()
                        .and_then(|s| Uuid::parse_str(s).ok());

                    let orchestrator_id: Option<String> = row.get("orchestrator_id");

                    let details = RunDetails {
                        summary: RunSummary {
                            run_id,
                            flow_name,
                            flow_id,
                            status,
                            items,
                            created_at,
                            completed_at,
                            root_run_id,
                            parent_run_id,
                            orchestrator_id,
                        },
                        item_details: Some(item_details),
                        overrides: None,
                    };

                    Ok(Some(details))
                },
                None => Ok(None),
            }
        }.boxed()
    }

    fn list_runs(
        &self,
        filters: &RunFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RunSummary>, StateError>> {
        let pool = self.pool.clone();
        let filters = filters.clone();

        async move {
            let mut sql = r#"
                SELECT
                    r.id, r.flow_name, r.flow_id, r.status,
                    r.created_at, r.completed_at,
                    r.root_run_id, r.parent_run_id, r.orchestrator_id,
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
                    ExecutionStatus::RecoveryFailed => "recoveryFailed",
                };
                conditions.push("r.status = ?".to_string());
                bind_values.push(status_str.to_string());
            }

            if let Some(ref flow_name) = filters.flow_name {
                conditions.push("r.flow_name = ?".to_string());
                bind_values.push(flow_name.clone());
            }

            if let Some(root_run_id) = filters.root_run_id {
                conditions.push("r.root_run_id = ?".to_string());
                bind_values.push(root_run_id.to_string());
            }

            if let Some(parent_run_id) = filters.parent_run_id {
                conditions.push("r.parent_run_id = ?".to_string());
                bind_values.push(parent_run_id.to_string());
            }

            // Filter for root runs only (no parent)
            if filters.roots_only == Some(true) {
                conditions.push("r.parent_run_id IS NULL".to_string());
            }

            // Filter by orchestrator_id
            if let Some(ref orch_filter) = filters.orchestrator_id {
                match orch_filter {
                    Some(orch_id) => {
                        conditions.push("r.orchestrator_id = ?".to_string());
                        bind_values.push(orch_id.clone());
                    }
                    None => {
                        conditions.push("r.orchestrator_id IS NULL".to_string());
                    }
                }
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
                    "cancelled" => ExecutionStatus::Cancelled,
                    "paused" => ExecutionStatus::Paused,
                    "recoveryFailed" => ExecutionStatus::RecoveryFailed,
                    _ => ExecutionStatus::Running,
                };

                let flow_name = row.get::<Option<String>, _>("flow_name");
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

                let root_run_id_str: Option<String> = row.get("root_run_id");
                let root_run_id = root_run_id_str
                    .as_ref()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or(run_id);

                let parent_run_id_str: Option<String> = row.get("parent_run_id");
                let parent_run_id = parent_run_id_str
                    .as_ref()
                    .and_then(|s| Uuid::parse_str(s).ok());

                let orchestrator_id: Option<String> = row.get("orchestrator_id");

                let summary = RunSummary {
                    run_id,
                    flow_name,
                    flow_id,
                    status,
                    items,
                    created_at,
                    completed_at,
                    root_run_id,
                    parent_run_id,
                    orchestrator_id,
                };

                summaries.push(summary);
            }

            Ok(summaries)
        }
        .boxed()
    }

    fn update_run_status(
        &self,
        run_id: Uuid,
        status: ExecutionStatus,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            let is_terminal = matches!(
                status,
                ExecutionStatus::Completed
                    | ExecutionStatus::Failed
                    | ExecutionStatus::Cancelled
                    | ExecutionStatus::RecoveryFailed
            );

            let sql = if is_terminal {
                "UPDATE runs SET status = ?, completed_at = CURRENT_TIMESTAMP WHERE id = ?"
            } else {
                "UPDATE runs SET status = ? WHERE id = ?"
            };

            sqlx::query(sql)
                .bind(status.as_str())
                .bind(run_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            if is_terminal {
                self.completion_notifier.notify_completion(run_id);
            }

            Ok(())
        }
        .boxed()
    }

    fn get_run_overrides(
        &self,
        _run_id: Uuid,
    ) -> BoxFuture<
        '_,
        error_stack::Result<Option<stepflow_core::workflow::WorkflowOverrides>, StateError>,
    > {
        async move { Ok(None) }.boxed()
    }

    fn record_item_result(
        &self,
        run_id: Uuid,
        item_index: usize,
        result: FlowResult,
        step_statuses: Vec<stepflow_dtos::StepStatusInfo>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            let status = match &result {
                FlowResult::Success(_) => "completed",
                FlowResult::Failed(_) => "failed",
            };

            let result_json =
                serde_json::to_string(&result).change_context(StateError::Serialization)?;

            let step_statuses_json =
                serde_json::to_string(&step_statuses).change_context(StateError::Serialization)?;

            let sql = r#"
                UPDATE run_items
                SET status = ?, result_json = ?, step_statuses_json = ?, completed_at = CURRENT_TIMESTAMP
                WHERE run_id = ? AND item_index = ?
            "#;

            let result = sqlx::query(sql)
                .bind(status)
                .bind(&result_json)
                .bind(&step_statuses_json)
                .bind(run_id.to_string())
                .bind(item_index as i64)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            if result.rows_affected() == 0 {
                return Err(error_stack::report!(StateError::RunNotFound { run_id }));
            }

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

                self.completion_notifier.notify_completion(run_id);
            }

            Ok(())
        }
        .boxed()
    }

    fn get_item_results(
        &self,
        run_id: Uuid,
        order: ResultOrder,
    ) -> BoxFuture<'_, error_stack::Result<Vec<ItemResult>, StateError>> {
        let pool = self.pool.clone();
        let run_id_str = run_id.to_string();

        async move {
            let order_clause = match order {
                ResultOrder::ByIndex => "ORDER BY item_index",
                ResultOrder::ByCompletion => {
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

    fn update_run_orchestrator(
        &self,
        run_id: Uuid,
        orchestrator_id: Option<String>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        async move {
            let sql = "UPDATE runs SET orchestrator_id = ? WHERE id = ?";
            let result = sqlx::query(sql)
                .bind(&orchestrator_id)
                .bind(run_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;
            if result.rows_affected() == 0 {
                return Err(
                    error_stack::Report::new(StateError::Internal).attach_printable(format!(
                        "update_run_orchestrator: no run found for id {run_id}"
                    )),
                );
            }
            Ok(())
        }
        .boxed()
    }

    fn orphan_runs_by_stale_orchestrators(
        &self,
        live_orchestrator_ids: &HashSet<String>,
    ) -> BoxFuture<'_, error_stack::Result<usize, StateError>> {
        let pool = self.pool.clone();
        let live_ids: Vec<String> = live_orchestrator_ids.iter().cloned().collect();
        async move {
            if live_ids.is_empty() {
                // No live orchestrators — orphan all running runs that have an orchestrator
                let sql = "UPDATE runs SET orchestrator_id = NULL WHERE orchestrator_id IS NOT NULL AND status = 'running'";
                let result = sqlx::query(sql)
                    .execute(&pool)
                    .await
                    .change_context(StateError::Internal)?;
                return Ok(result.rows_affected() as usize);
            }

            // Build a parameterized IN clause for the live IDs
            let placeholders: Vec<&str> = live_ids.iter().map(|_| "?").collect();
            let sql = format!(
                "UPDATE runs SET orchestrator_id = NULL WHERE orchestrator_id IS NOT NULL AND orchestrator_id NOT IN ({}) AND status = 'running'",
                placeholders.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for id in &live_ids {
                query = query.bind(id);
            }
            let result = query
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;
            Ok(result.rows_affected() as usize)
        }
        .boxed()
    }

    fn wait_for_completion(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT status FROM runs WHERE id = ?";

            let receiver = self.completion_notifier.subscribe();

            let row = sqlx::query(sql)
                .bind(run_id.to_string())
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)?;

            match row {
                Some(row) => {
                    let status_str: String = row.get("status");
                    if matches!(status_str.as_str(), "completed" | "failed" | "cancelled") {
                        return Ok(());
                    }
                }
                None => {
                    return Err(error_stack::report!(StateError::RunNotFound { run_id }));
                }
            }

            self.completion_notifier
                .wait_for_completion_with_receiver(run_id, receiver)
                .await;

            loop {
                let row = sqlx::query(sql)
                    .bind(run_id.to_string())
                    .fetch_optional(&pool)
                    .await
                    .change_context(StateError::Internal)?;

                match row {
                    Some(row) => {
                        let status_str: String = row.get("status");
                        if matches!(status_str.as_str(), "completed" | "failed" | "cancelled") {
                            return Ok(());
                        }
                    }
                    None => {
                        return Err(error_stack::report!(StateError::RunNotFound { run_id }));
                    }
                }

                tokio::task::yield_now().await;
            }
        }
        .boxed()
    }
}

impl ExecutionJournal for SqliteStateStore {
    fn append(
        &self,
        entry: JournalEntry,
    ) -> BoxFuture<'_, error_stack::Result<SequenceNumber, StateError>> {
        let pool = self.pool.clone();

        async move {
            // Get the next sequence number for this root run's journal.
            // All events for an execution tree share the same journal keyed by root_run_id.
            let max_seq_sql =
                "SELECT COALESCE(MAX(sequence), -1) as max_seq FROM journal_entries WHERE root_run_id = ?";
            let row = sqlx::query(max_seq_sql)
                .bind(entry.root_run_id.to_string())
                .fetch_one(&pool)
                .await
                .change_context(StateError::Internal)?;

            let max_seq: i64 = row.get("max_seq");
            let next_seq = SequenceNumber::new((max_seq + 1) as u64);

            // Serialize the event
            let event_type = match &entry.event {
                stepflow_state::JournalEvent::RunCreated { .. } => "run_created",
                stepflow_state::JournalEvent::RunInitialized { .. } => "run_initialized",
                stepflow_state::JournalEvent::RunCompleted { .. } => "run_completed",
                stepflow_state::JournalEvent::TaskCompleted { .. } => "task_completed",
                stepflow_state::JournalEvent::StepsUnblocked { .. } => "steps_unblocked",
                stepflow_state::JournalEvent::ItemCompleted { .. } => "item_completed",
            };

            let event_data =
                serde_json::to_string(&entry.event).change_context(StateError::Serialization)?;

            let timestamp = entry.timestamp.to_rfc3339();

            let insert_sql = r#"
                INSERT INTO journal_entries (run_id, sequence, root_run_id, timestamp, event_type, event_data)
                VALUES (?, ?, ?, ?, ?, ?)
            "#;

            sqlx::query(insert_sql)
                .bind(entry.run_id.to_string())
                .bind(next_seq.value() as i64)
                .bind(entry.root_run_id.to_string())
                .bind(&timestamp)
                .bind(event_type)
                .bind(&event_data)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(next_seq)
        }
        .boxed()
    }

    fn read_from(
        &self,
        root_run_id: Uuid,
        from_sequence: SequenceNumber,
        limit: usize,
    ) -> BoxFuture<'_, error_stack::Result<Vec<(SequenceNumber, JournalEntry)>, StateError>> {
        let pool = self.pool.clone();

        async move {
            // Read all entries for this root run's journal (across all runs in the tree)
            let sql = r#"
                SELECT run_id, sequence, timestamp, event_data
                FROM journal_entries
                WHERE root_run_id = ? AND sequence >= ?
                ORDER BY sequence
                LIMIT ?
            "#;

            let rows = sqlx::query(sql)
                .bind(root_run_id.to_string())
                .bind(from_sequence.value() as i64)
                .bind(limit as i64)
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut entries = Vec::new();
            for row in rows {
                let sequence = SequenceNumber::new(row.get::<i64, _>("sequence") as u64);
                let run_id_str: String = row.get("run_id");
                let run_id = Uuid::parse_str(&run_id_str).change_context(StateError::Internal)?;
                let timestamp_str: String = row.get("timestamp");
                let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                    .change_context(StateError::Internal)?
                    .with_timezone(&chrono::Utc);
                let event_data: String = row.get("event_data");
                let event: stepflow_state::JournalEvent =
                    serde_json::from_str(&event_data).change_context(StateError::Serialization)?;

                let entry = JournalEntry {
                    run_id,
                    root_run_id,
                    timestamp,
                    event,
                };

                entries.push((sequence, entry));
            }

            Ok(entries)
        }
        .boxed()
    }

    fn latest_sequence(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<SequenceNumber>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT MAX(sequence) as max_seq FROM journal_entries WHERE root_run_id = ?";

            let row = sqlx::query(sql)
                .bind(root_run_id.to_string())
                .fetch_one(&pool)
                .await
                .change_context(StateError::Internal)?;

            let max_seq: Option<i64> = row.get("max_seq");
            Ok(max_seq.map(|s| SequenceNumber::new(s as u64)))
        }
        .boxed()
    }

    fn list_active_roots(
        &self,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RootJournalInfo>, StateError>> {
        let pool = self.pool.clone();

        async move {
            // Get all root runs that have journal entries, along with their statistics.
            // Group by root_run_id since all events for an execution tree share one journal.
            let sql = r#"
                SELECT
                    root_run_id,
                    MAX(sequence) as latest_sequence,
                    COUNT(*) as entry_count
                FROM journal_entries
                GROUP BY root_run_id
            "#;

            let rows = sqlx::query(sql)
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut infos = Vec::new();
            for row in rows {
                let root_run_id_str: String = row.get("root_run_id");
                let root_run_id =
                    Uuid::parse_str(&root_run_id_str).change_context(StateError::Internal)?;

                let latest_sequence =
                    SequenceNumber::new(row.get::<i64, _>("latest_sequence") as u64);

                let entry_count = row.get::<i64, _>("entry_count") as u64;

                infos.push(RootJournalInfo {
                    root_run_id,
                    latest_sequence,
                    entry_count,
                });
            }

            Ok(infos)
        }
        .boxed()
    }

    fn flush(&self, _root_run_id: Uuid) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        // SQLite with WAL mode commits each INSERT immediately.
        // Future implementations may batch writes and require actual flushing.
        async move { Ok(()) }.boxed()
    }
}
