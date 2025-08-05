// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
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
    RunDetails, RunFilters, RunSummary, StateError, StateStore, StateWriteOperation, StepInfo,
    StepResult, WorkflowLabelMetadata, WorkflowWithMetadata,
};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::migrations;

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
                        tracing::error!("Failed to record step result: {:?}", e);
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
                        tracing::error!("Failed to update step statuses: {:?}", e);
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
        run_id: Uuid,
        flow_id: BlobId,
        flow_name: Option<String>,
        flow_label: Option<String>,
        debug_mode: bool,
        input: ValueRef,
    ) -> Result<(), StateError> {
        let input_json =
            serde_json::to_string(input.as_ref()).change_context(StateError::Serialization)?;
        let sql = "INSERT INTO runs (id, flow_id, flow_name, flow_label, status, debug_mode, input_json) VALUES (?, ?, ?, ?, 'running', ?, ?)";

        sqlx::query(sql)
            .bind(run_id.to_string())
            .bind(flow_id.to_string())
            .bind(flow_name)
            .bind(flow_label)
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
                    tracing::warn!("Unknown blob type '{}', defaulting to 'data'", type_str);
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

    fn get_flows(
        &self,
        name: &str,
    ) -> BoxFuture<'_, error_stack::Result<Vec<(BlobId, chrono::DateTime<chrono::Utc>)>, StateError>>
    {
        let pool = self.pool.clone();
        let name = name.to_string();

        async move {
            // Query both labeled flows and flows with matching names directly from blob data
            let sql = r#"
                SELECT DISTINCT b.id, b.created_at 
                FROM blobs b 
                LEFT JOIN flow_labels l ON l.flow_id = b.id 
                WHERE b.blob_type = 'flow' 
                AND (
                    l.name = ? 
                    OR json_extract(b.data, '$.name') = ?
                )
                ORDER BY b.created_at DESC
            "#;

            let rows = sqlx::query(sql)
                .bind(&name)
                .bind(&name)
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut results = Vec::new();
            for row in rows {
                let flow_id =
                    BlobId::new(row.get::<String, _>("id")).change_context(StateError::Internal)?;
                let created_at =
                    chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))
                        .change_context(StateError::Internal)?
                        .with_timezone(&chrono::Utc);
                results.push((flow_id, created_at));
            }

            Ok(results)
        }
        .boxed()
    }

    fn get_named_flow(
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
                    let sql = r#"
                        SELECT l.name, l.label, l.flow_id, l.created_at, l.updated_at, 
                               b.data, b.created_at as flow_created_at
                        FROM flow_labels l 
                        JOIN blobs b ON l.flow_id = b.id 
                        WHERE l.name = ? AND l.label = ? AND b.blob_type = 'flow'
                    "#;

                    let row = sqlx::query(sql)
                        .bind(&name)
                        .bind(&label_str)
                        .fetch_optional(&pool)
                        .await
                        .change_context(StateError::Internal)?;

                    match row {
                        Some(row) => {
                            let workflow_content: String = row.get("data");
                            let workflow: Flow = serde_json::from_str(&workflow_content)
                                .change_context(StateError::Internal)?;
                            let flow_id = BlobId::new(row.get::<String, _>("flow_id"))
                                .change_context(StateError::Internal)?;
                            let label_metadata = WorkflowLabelMetadata {
                                name: row.get("name"),
                                label: row.get("label"),
                                flow_id: flow_id.clone(),
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
                            let created_at = chrono::DateTime::parse_from_rfc3339(
                                &row.get::<String, _>("flow_created_at"),
                            )
                            .change_context(StateError::Internal)?
                            .with_timezone(&chrono::Utc);

                            Ok(Some(WorkflowWithMetadata {
                                workflow: Arc::new(workflow),
                                flow_id,
                                created_at,
                                label_info: Some(label_metadata),
                            }))
                        }
                        None => Ok(None),
                    }
                }
                None => {
                    // Get latest workflow by name
                    let sql = r#"
                        SELECT DISTINCT b.id, b.data, b.created_at 
                        FROM blobs b 
                        LEFT JOIN flow_labels l ON l.flow_id = b.id 
                        WHERE b.blob_type = 'flow' 
                        AND (
                            l.name = ? 
                            OR json_extract(b.data, '$.name') = ?
                        )
                        ORDER BY b.created_at DESC 
                        LIMIT 1
                    "#;

                    let row = sqlx::query(sql)
                        .bind(&name)
                        .bind(&name)
                        .fetch_optional(&pool)
                        .await
                        .change_context(StateError::Internal)?;

                    match row {
                        Some(row) => {
                            let workflow_content: String = row.get("data");
                            let workflow: Flow = serde_json::from_str(&workflow_content)
                                .change_context(StateError::Internal)?;
                            let flow_id = BlobId::new(row.get::<String, _>("id"))
                                .change_context(StateError::Internal)?;
                            let created_at = chrono::DateTime::parse_from_rfc3339(
                                &row.get::<String, _>("created_at"),
                            )
                            .change_context(StateError::Internal)?
                            .with_timezone(&chrono::Utc);

                            Ok(Some(WorkflowWithMetadata {
                                workflow: Arc::new(workflow),
                                flow_id,
                                created_at,
                                label_info: None,
                            }))
                        }
                        None => Ok(None),
                    }
                }
            }
        }
        .boxed()
    }

    fn create_or_update_label(
        &self,
        name: &str,
        label: &str,
        flow_id: BlobId,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let label = label.to_string();

        async move {
            let sql = "INSERT OR REPLACE INTO flow_labels (name, label, flow_id, updated_at) VALUES (?, ?, ?, CURRENT_TIMESTAMP)";

            sqlx::query(sql)
                .bind(&name)
                .bind(&label)
                .bind(flow_id.to_string())
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
            let sql = "SELECT name, label, flow_id, created_at, updated_at FROM flow_labels WHERE name = ? ORDER BY created_at DESC";

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
                    flow_id: BlobId::new(row.get::<String, _>("flow_id")).change_context(StateError::Internal)?,
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

    fn list_flow_names(&self) -> BoxFuture<'_, error_stack::Result<Vec<String>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = r#"
                SELECT DISTINCT name FROM flow_labels 
                UNION 
                SELECT DISTINCT json_extract(data, '$.name') as name 
                FROM blobs 
                WHERE blob_type = 'flow' 
                AND json_extract(data, '$.name') IS NOT NULL 
                ORDER BY name
            "#;

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
        }
        .boxed()
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
            let sql = "DELETE FROM flow_labels WHERE name = ? AND label = ?";

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

    fn create_run(
        &self,
        run_id: Uuid,
        flow_id: BlobId,
        flow_name: Option<&str>,
        flow_label: Option<&str>,
        debug_mode: bool,
        input: ValueRef,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        // Execute synchronously to avoid race condition with step results
        let pool = self.pool.clone();
        let workflow_name = flow_name.map(|s| s.to_string());
        let workflow_label = flow_label.map(|s| s.to_string());

        async move {
            Self::create_run_sync(
                &pool,
                run_id,
                flow_id,
                workflow_name,
                workflow_label,
                debug_mode,
                input,
            )
            .await
        }
        .boxed()
    }

    fn update_run_status(
        &self,
        run_id: Uuid,
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
                .bind(run_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }.boxed()
    }

    fn get_run(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<RunDetails>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT id, flow_name, flow_label, flow_id, status, debug_mode, input_json, result_json, created_at, completed_at FROM runs WHERE id = ?";

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
                            tracing::warn!("Unrecognized execution status: {status_str}");
                            ExecutionStatus::Running
                        }
                    };

                    let flow_name = row.get::<Option<String>, _>("flow_name");
                    let flow_label = row.get::<Option<String>, _>("flow_label");
                    let flow_id = BlobId::new(row.get::<String, _>("flow_id")).change_context(StateError::Internal)?;

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

                    let details = RunDetails {
                        summary: RunSummary {
                            run_id,
                            flow_name,
                            flow_label,
                            flow_id,
                            status,
                            debug_mode: row.get("debug_mode"),
                            created_at,
                            completed_at,
                        },
                        input,
                        result: result.map(FlowResult::Success),
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
            let mut sql = "SELECT id, flow_name, flow_label, flow_id, status, debug_mode, created_at, completed_at FROM runs".to_string();
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

            if let Some(ref flow_name) = filters.flow_name {
                conditions.push("flow_name = ?".to_string());
                bind_values.push(flow_name.clone());
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
                let run_id_str: String = row.get("id");
                let run_id =
                    Uuid::parse_str(&run_id_str).change_context(StateError::Internal)?;

                let status_str: String = row.get("status");
                let status = match status_str.as_str() {
                    "running" => ExecutionStatus::Running,
                    "completed" => ExecutionStatus::Completed,
                    "failed" => ExecutionStatus::Failed,
                    "paused" => ExecutionStatus::Paused,
                    _ => ExecutionStatus::Running,
                };

                let flow_name = row.get::<Option<String>, _>("workflow_name");
                let flow_label = row.get::<Option<String>, _>("workflow_label");
                let flow_id = BlobId::new(row.get::<String, _>("flow_id")).change_context(StateError::Internal)?;

                let summary = RunSummary {
                    run_id,
                    flow_name,
                    flow_label,
                    flow_id,
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
            tracing::error!("Failed to queue step status update: {:?}", e);
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
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>> {
        let pool = self.pool.clone();

        async move {
            // Simply return steps with 'runnable' status - dependency checking is done by the caller
            let sql = r#"
                SELECT step_index, step_id, component, status, created_at, updated_at
                FROM step_info
                WHERE run_id = ? AND status = 'runnable'
                ORDER BY step_index
            "#;

            let rows = sqlx::query(sql)
                .bind(run_id.to_string())
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)?;

            let mut runnable_steps = Vec::new();
            for row in rows {
                let component_str: String = row.get("component");
                let component = Component::from_string(&component_str);

                let step_info = StepInfo {
                    run_id,
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
