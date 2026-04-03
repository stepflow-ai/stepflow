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
use sqlx::{
    FromRow, QueryBuilder, Sqlite, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{BlobId, BlobMetadata, BlobType, FlowResult};
use stepflow_domain::{
    ItemResult, ItemStatistics, ResultOrder, RunFilters, RunSummary, StepStatusEntry,
};
use stepflow_state::{
    BlobStore, CheckpointStore, ExecutionJournal, JournalEntry, JournalEvent, MetadataStore,
    RawBlob, RootJournalInfo, RunCompletionNotifier, SequenceNumber, StateError, StoredCheckpoint,
};
use uuid::Uuid;

use crate::migrations;

/// Map u64 → i64 preserving ordering (for SQLite INTEGER columns).
///
/// XORs the sign bit so that `0u64 → i64::MIN` and `u64::MAX → i64::MAX`.
/// This is a bijection: every u64 maps to a unique i64 and the relative
/// ordering of any two u64 values is preserved in the i64 domain, which
/// means SQLite `>=` / `ORDER BY` comparisons work correctly.
fn u64_to_sql(v: u64) -> i64 {
    (v ^ (1u64 << 63)) as i64
}

/// Inverse of [`u64_to_sql`]: map i64 back to u64 preserving ordering.
fn sql_to_u64(v: i64) -> u64 {
    (v as u64) ^ (1u64 << 63)
}

/// Parse a `StepStatus` from a database string.
fn parse_step_status(s: &str) -> StepStatus {
    s.parse().unwrap_or_else(|_| {
        log::warn!("Unrecognized step status: {s}, defaulting to blocked");
        StepStatus::Blocked
    })
}

/// Parse an `ExecutionStatus` from a database string.
fn parse_execution_status(s: &str) -> ExecutionStatus {
    match s {
        "running" => ExecutionStatus::Running,
        "completed" => ExecutionStatus::Completed,
        "failed" => ExecutionStatus::Failed,
        "cancelled" => ExecutionStatus::Cancelled,
        "paused" => ExecutionStatus::Paused,
        "recoveryFailed" => ExecutionStatus::RecoveryFailed,
        _ => {
            log::warn!("Unrecognized execution status: {s}");
            ExecutionStatus::Running
        }
    }
}

// =========================================================================
// FromRow structs — typed column extraction for all SQL queries.
//
// sqlx::FromRow derives try_get() for every field, replacing manual
// row.get() calls that panic on decode errors. Field names must match
// SQL column names (or use #[sqlx(rename = "...")]).
// =========================================================================

#[derive(FromRow)]
struct BlobRow {
    data: Vec<u8>,
    blob_type: String,
    filename: Option<String>,
}

#[derive(FromRow)]
struct RunRow {
    status: String,
    flow_name: Option<String>,
    flow_id: String,
    created_at: String,
    completed_at: Option<String>,
    root_run_id: Option<String>,
    parent_run_id: Option<String>,
    orchestrator_id: Option<String>,
    created_at_seqno: i64,
    finished_at_seqno: Option<i64>,
}

#[derive(FromRow)]
struct RunStatRow {
    status: String,
    cnt: i64,
}

#[derive(FromRow)]
struct ListRunsRow {
    id: String,
    status: String,
    flow_name: Option<String>,
    flow_id: String,
    created_at: String,
    completed_at: Option<String>,
    root_run_id: Option<String>,
    parent_run_id: Option<String>,
    orchestrator_id: Option<String>,
    created_at_seqno: i64,
    finished_at_seqno: Option<i64>,
    item_total: i64,
    item_running: i64,
    item_completed: i64,
    item_failed: i64,
    item_cancelled: i64,
}

#[derive(FromRow)]
struct ItemResultRow {
    item_index: i64,
    status: String,
    result_json: Option<String>,
    completed_at: Option<String>,
}

#[derive(FromRow)]
struct ItemCountsRow {
    total: i64,
    done: i64,
    failed: i64,
}

#[derive(FromRow)]
struct StepStatusRow {
    step_id: String,
    step_index: i64,
    item_index: i64,
    status: String,
    component: Option<String>,
    result_json: Option<String>,
    journal_seqno: i64,
    updated_at: Option<String>,
}

#[derive(FromRow)]
struct RunStatusRow {
    status: String,
}

#[derive(FromRow)]
struct ComponentRegistrationRow {
    plugin: String,
    component_id: String,
    path: String,
    description: Option<String>,
    input_schema_json: Option<String>,
    output_schema_json: Option<String>,
    last_updated: String,
}

#[derive(FromRow)]
struct JournalEntryRow {
    sequence: i64,
    run_id: String,
    timestamp: String,
    event_data: String,
}

#[derive(FromRow)]
struct MaxSeqRow {
    max_seq: Option<i64>,
}

#[derive(FromRow)]
struct RootJournalRow {
    root_run_id: String,
    latest_sequence: i64,
    entry_count: i64,
}

#[derive(FromRow)]
struct CheckpointRow {
    sequence_number: i64,
    data: Vec<u8>,
}

// =========================================================================
// FromRow → domain type conversions
// =========================================================================

impl StepStatusRow {
    fn into_entry(self) -> error_stack::Result<StepStatusEntry, StateError> {
        let result = self
            .result_json
            .map(|json| serde_json::from_str::<FlowResult>(&json))
            .transpose()
            .change_context(StateError::Serialization)
            .attach_printable_lazy(|| {
                format!(
                    "failed to deserialize result_json for step '{}'",
                    self.step_id
                )
            })?;

        let updated_at = self
            .updated_at
            .as_deref()
            .and_then(parse_sqlite_datetime)
            .unwrap_or_else(|| {
                log::warn!(
                    "Missing or unparseable updated_at for step '{}'",
                    self.step_id
                );
                chrono::Utc::now()
            });

        Ok(StepStatusEntry {
            step_id: self.step_id,
            step_index: self.step_index as usize,
            item_index: self.item_index as u32,
            status: parse_step_status(&self.status),
            component: self.component,
            result,
            journal_seqno: sql_to_u64(self.journal_seqno),
            updated_at,
        })
    }
}

impl RunRow {
    fn into_summary(
        self,
        run_id: Uuid,
        items: ItemStatistics,
    ) -> error_stack::Result<RunSummary, StateError> {
        let flow_id = BlobId::new(self.flow_id).change_context(StateError::Internal)?;
        let created_at = parse_sqlite_datetime(&self.created_at).ok_or_else(|| {
            error_stack::report!(StateError::Internal)
                .attach_printable(format!("unparseable created_at: {}", self.created_at))
        })?;
        let completed_at = self.completed_at.as_deref().and_then(parse_sqlite_datetime);
        let root_run_id = self
            .root_run_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or(run_id);
        let parent_run_id = self
            .parent_run_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok());

        Ok(RunSummary {
            run_id,
            flow_name: self.flow_name,
            flow_id,
            status: parse_execution_status(&self.status),
            items,
            created_at,
            completed_at,
            root_run_id,
            parent_run_id,
            orchestrator_id: self.orchestrator_id,
            created_at_seqno: sql_to_u64(self.created_at_seqno),
            finished_at_seqno: self.finished_at_seqno.map(sql_to_u64),
        })
    }
}

impl ListRunsRow {
    fn into_summary(self) -> error_stack::Result<RunSummary, StateError> {
        let run_id = Uuid::parse_str(&self.id).change_context(StateError::Internal)?;
        let flow_id = BlobId::new(self.flow_id).change_context(StateError::Internal)?;
        let created_at = parse_sqlite_datetime(&self.created_at)
            .ok_or_else(|| error_stack::report!(StateError::Internal))?;
        let completed_at = self.completed_at.as_deref().and_then(parse_sqlite_datetime);
        let root_run_id = self
            .root_run_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or(run_id);
        let parent_run_id = self
            .parent_run_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok());

        Ok(RunSummary {
            run_id,
            flow_name: self.flow_name,
            flow_id,
            status: parse_execution_status(&self.status),
            items: ItemStatistics {
                total: self.item_total as usize,
                running: self.item_running as usize,
                completed: self.item_completed as usize,
                failed: self.item_failed as usize,
                cancelled: self.item_cancelled as usize,
            },
            created_at,
            completed_at,
            root_run_id,
            parent_run_id,
            orchestrator_id: self.orchestrator_id,
            created_at_seqno: sql_to_u64(self.created_at_seqno),
            finished_at_seqno: self.finished_at_seqno.map(sql_to_u64),
        })
    }
}

impl ItemResultRow {
    fn into_item_result(self) -> error_stack::Result<ItemResult, StateError> {
        let result = self
            .result_json
            .map(|json| serde_json::from_str::<FlowResult>(&json))
            .transpose()
            .change_context(StateError::Serialization)
            .attach_printable_lazy(|| {
                format!(
                    "failed to deserialize result_json for item {}",
                    self.item_index
                )
            })?;
        let completed_at = self.completed_at.as_deref().and_then(parse_sqlite_datetime);

        Ok(ItemResult {
            item_index: self.item_index as usize,
            status: parse_execution_status(&self.status),
            result,
            completed_at,
        })
    }
}

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
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
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
    pub(crate) pool: SqlitePool,
    /// Notifier for run completion events
    completion_notifier: RunCompletionNotifier,
}

impl SqliteStateStore {
    /// Create a new SqliteStateStore with the given configuration.
    ///
    /// If `auto_migrate` is true, runs all migrations (blob, metadata, journal).
    /// For selective initialization, set `auto_migrate` to false and call the
    /// trait-level `initialize_*` methods instead.
    pub async fn new(config: SqliteStateStoreConfig) -> Result<Self, StateError> {
        let connect_options = config
            .database_url
            .parse::<SqliteConnectOptions>()
            .map_err(|e| {
                error_stack::report!(StateError::Connection).attach_printable(format!(
                    "Invalid database URL '{}': {e}",
                    config.database_url
                ))
            })?;

        let connect_options = connect_options
            // WAL mode allows concurrent readers during writes — the facade
            // polls for status while the execution engine writes step results.
            .pragma("journal_mode", "WAL")
            // Writers wait up to 5s instead of failing immediately on contention.
            .pragma("busy_timeout", "5000")
            // Store temporary tables and indices in memory. Without this,
            // SQLite tries to create temp files in /tmp which fails with
            // SQLITE_IOERR_GETTEMPPATH (6410) when the container has
            // readOnlyRootFilesystem: true.
            .pragma("temp_store", "MEMORY");

        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .connect_with(connect_options)
            .await
            .change_context(StateError::Connection)
            .attach_printable_lazy(|| format!("Database URL: {}", config.database_url))?;

        if config.auto_migrate {
            migrations::run_all_migrations(&pool).await?;
        }

        Ok(Self {
            pool,
            completion_notifier: RunCompletionNotifier::new(),
        })
    }

    /// Create SqliteStateStore directly from a database URL.
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
        let sql = "INSERT OR IGNORE INTO runs (id, flow_id, flow_name, status, overrides_json, root_run_id, parent_run_id, orchestrator_id, created_at_seqno) VALUES (?, ?, ?, 'running', ?, ?, ?, ?, ?)";

        let created_at_seqno: i64 = u64_to_sql(params.created_at_seqno.value());

        sqlx::query(sql)
            .bind(params.run_id.to_string())
            .bind(params.flow_id.to_string())
            .bind(&params.workflow_name)
            .bind(&overrides_json)
            .bind(params.root_run_id.to_string())
            .bind(params.parent_run_id.map(|id| id.to_string()))
            .bind(&params.orchestrator_id)
            .bind(created_at_seqno)
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
    fn initialize_blob_store(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        async move { migrations::run_blob_migrations(&pool).await }.boxed()
    }

    fn put_blob(
        &self,
        content: &[u8],
        blob_type: BlobType,
        metadata: BlobMetadata,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        let content = content.to_vec();
        async move {
            let blob_id = BlobId::from_binary(&content).change_context(StateError::Internal)?;

            let type_str = match blob_type {
                BlobType::Flow => "flow",
                BlobType::Data => "data",
                BlobType::Binary => "binary",
            };

            // Store raw bytes directly in BLOB column
            let sql =
                "INSERT OR IGNORE INTO blobs (id, data, blob_type, filename) VALUES (?, ?, ?, ?)";

            sqlx::query(sql)
                .bind(blob_id.as_str())
                .bind(&content)
                .bind(type_str)
                .bind(&metadata.filename)
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
    ) -> BoxFuture<'_, error_stack::Result<Option<RawBlob>, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            let sql = "SELECT data, blob_type, filename FROM blobs WHERE id = ?";

            let row = sqlx::query_as::<_, BlobRow>(sql)
                .bind(blob_id.as_str())
                .fetch_optional(&self.pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| format!("failed to fetch blob '{}'", blob_id.as_str()))?;

            let Some(row) = row else {
                return Ok(None);
            };

            let blob_type = match row.blob_type.as_str() {
                "flow" => BlobType::Flow,
                "data" => BlobType::Data,
                "binary" => BlobType::Binary,
                _ => {
                    log::warn!(
                        "Unknown blob type '{}', defaulting to 'data'",
                        row.blob_type
                    );
                    BlobType::Data
                }
            };

            Ok(Some(RawBlob {
                content: row.data,
                blob_type,
                metadata: BlobMetadata {
                    filename: row.filename,
                },
            }))
        }
        .boxed()
    }
}

impl MetadataStore for SqliteStateStore {
    fn initialize_metadata_store(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        async move { migrations::run_metadata_migrations(&pool).await }.boxed()
    }

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
    ) -> BoxFuture<'_, error_stack::Result<Option<RunSummary>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT flow_name, flow_id, status, created_at, completed_at, root_run_id, parent_run_id, orchestrator_id, created_at_seqno, finished_at_seqno FROM runs WHERE id = ?";

            let row = sqlx::query_as::<_, RunRow>(sql)
                .bind(run_id.to_string())
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to fetch run '{run_id}'")
                })?;

            match row {
                Some(run_row) => {
                    // Compute item statistics from the run_items table
                    let items_sql = "SELECT status, COUNT(*) as cnt FROM run_items WHERE run_id = ? GROUP BY status";
                    let stat_rows = sqlx::query_as::<_, RunStatRow>(items_sql)
                        .bind(run_id.to_string())
                        .fetch_all(&pool)
                        .await
                        .change_context(StateError::Internal)
                        .attach_printable_lazy(|| {
                            format!("failed to fetch item stats for run '{run_id}'")
                        })?;

                    let mut running = 0usize;
                    let mut completed_count = 0usize;
                    let mut failed = 0usize;
                    let mut cancelled = 0usize;
                    let mut total = 0usize;

                    for stat_row in stat_rows {
                        let cnt = stat_row.cnt as usize;
                        total += cnt;
                        match stat_row.status.as_str() {
                            "running" => running += cnt,
                            "completed" => completed_count += cnt,
                            "failed" => failed += cnt,
                            "cancelled" => cancelled += cnt,
                            _ => running += cnt,
                        }
                    }

                    let items = ItemStatistics {
                        total,
                        running,
                        completed: completed_count,
                        failed,
                        cancelled,
                    };

                    Ok(Some(run_row.into_summary(run_id, items)?))
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
            let mut qb = QueryBuilder::<Sqlite>::new(
                r#"SELECT
                    r.id, r.flow_name, r.flow_id, r.status,
                    r.created_at, r.completed_at,
                    r.root_run_id, r.parent_run_id, r.orchestrator_id,
                    r.created_at_seqno, r.finished_at_seqno,
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
                ) i ON r.id = i.run_id WHERE 1=1"#,
            );

            if let Some(ref status) = filters.status {
                qb.push(" AND r.status = ");
                qb.push_bind(status.as_str().to_string());
            }

            if let Some(ref flow_name) = filters.flow_name {
                qb.push(" AND r.flow_name = ");
                qb.push_bind(flow_name.clone());
            }

            if let Some(root_run_id) = filters.root_run_id {
                qb.push(" AND r.root_run_id = ");
                qb.push_bind(root_run_id.to_string());
            }

            if let Some(parent_run_id) = filters.parent_run_id {
                qb.push(" AND r.parent_run_id = ");
                qb.push_bind(parent_run_id.to_string());
            }

            // Filter for root runs only (no parent)
            if filters.roots_only == Some(true) {
                qb.push(" AND r.parent_run_id IS NULL");
            }

            // Filter by orchestrator_id
            if let Some(ref orch_filter) = filters.orchestrator_id {
                match orch_filter {
                    Some(orch_id) => {
                        qb.push(" AND r.orchestrator_id = ");
                        qb.push_bind(orch_id.clone());
                    }
                    None => {
                        qb.push(" AND r.orchestrator_id IS NULL");
                    }
                }
            }

            // Filter by created_after_seqno (journal sequence ordering).
            // Convert u64 → i64 via u64_to_sql (order-preserving) so the SQL >=
            // comparison operates on the same encoding used at insert time.
            if let Some(offset_gte) = filters.created_after_seqno {
                qb.push(" AND r.created_at_seqno >= ");
                qb.push_bind(u64_to_sql(offset_gte));
            }

            // Filter: runs that haven't finished before the given seqno.
            // Semantics: still running (NULL) OR finished at/after this point.
            if let Some(seqno) = filters.not_finished_before_seqno {
                qb.push(" AND (r.finished_at_seqno IS NULL OR r.finished_at_seqno >= ");
                qb.push_bind(u64_to_sql(seqno));
                qb.push(")");
            }

            qb.push(" ORDER BY r.created_at DESC");

            if let Some(limit) = filters.limit {
                qb.push(" LIMIT ");
                qb.push_bind(limit as i64);

                if let Some(offset) = filters.offset {
                    qb.push(" OFFSET ");
                    qb.push_bind(offset as i64);
                }
            }

            let rows = qb
                .build_query_as::<ListRunsRow>()
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| "failed to fetch runs list")?;

            let mut summaries = Vec::with_capacity(rows.len());
            for row in rows {
                summaries.push(row.into_summary()?);
            }

            Ok(summaries)
        }
        .boxed()
    }

    fn update_run_status(
        &self,
        run_id: Uuid,
        status: ExecutionStatus,
        finished_at_seqno: Option<stepflow_state::SequenceNumber>,
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

            if let Some(seqno) = finished_at_seqno.filter(|_| is_terminal) {
                let seqno_sql = u64_to_sql(seqno.value());
                sqlx::query(
                    "UPDATE runs SET status = ?, completed_at = CURRENT_TIMESTAMP, \
                     finished_at_seqno = ? WHERE id = ?",
                )
                .bind(status.as_str())
                .bind(seqno_sql)
                .bind(run_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;
            } else if is_terminal {
                sqlx::query(
                    "UPDATE runs SET status = ?, completed_at = CURRENT_TIMESTAMP WHERE id = ?",
                )
                .bind(status.as_str())
                .bind(run_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;
            } else {
                sqlx::query("UPDATE runs SET status = ? WHERE id = ?")
                    .bind(status.as_str())
                    .bind(run_id.to_string())
                    .execute(&pool)
                    .await
                    .change_context(StateError::Internal)?;
            }

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
        step_statuses: Vec<stepflow_domain::StepStatusInfo>,
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

            let counts = sqlx::query_as::<_, ItemCountsRow>(count_sql)
                .bind(run_id.to_string())
                .bind(run_id.to_string())
                .bind(run_id.to_string())
                .fetch_one(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to fetch item counts for run '{run_id}'")
                })?;

            let total = counts.total;
            let done = counts.done;
            let failed = counts.failed;

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

            let rows = sqlx::query_as::<_, ItemResultRow>(&sql)
                .bind(&run_id_str)
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to fetch item results for run '{run_id_str}'")
                })?;

            let mut items = Vec::with_capacity(rows.len());
            for row in rows {
                items.push(row.into_item_result()?);
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

    fn update_step_status(
        &self,
        run_id: Uuid,
        item_index: usize,
        step_id: &str,
        step_index: usize,
        status: StepStatus,
        component: Option<&str>,
        result: Option<FlowResult>,
        journal_seqno: SequenceNumber,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let step_id = step_id.to_string();
        let component = component.map(|s| s.to_string());

        async move {
            let status_str = status.to_string();
            let result_json = result
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .change_context(StateError::Serialization)?;
            let seqno_sql = u64_to_sql(journal_seqno.value());

            let sql = r#"
                INSERT INTO step_statuses (run_id, item_index, step_id, step_index, status, component, result_json, journal_seqno)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(run_id, item_index, step_id) DO UPDATE SET
                    status = excluded.status,
                    component = COALESCE(excluded.component, step_statuses.component),
                    result_json = COALESCE(excluded.result_json, step_statuses.result_json),
                    journal_seqno = excluded.journal_seqno,
                    updated_at = CURRENT_TIMESTAMP
            "#;

            sqlx::query(sql)
                .bind(run_id.to_string())
                .bind(item_index as i64)
                .bind(&step_id)
                .bind(step_index as i64)
                .bind(status_str)
                .bind(&component)
                .bind(&result_json)
                .bind(seqno_sql)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }
        .boxed()
    }

    fn get_step_status(
        &self,
        run_id: Uuid,
        item_index: usize,
        step_id: &str,
    ) -> BoxFuture<'_, error_stack::Result<Option<StepStatusEntry>, StateError>> {
        let pool = self.pool.clone();
        let step_id = step_id.to_string();

        async move {
            let sql = "SELECT step_id, step_index, item_index, status, component, result_json, journal_seqno, updated_at \
                        FROM step_statuses WHERE run_id = ? AND item_index = ? AND step_id = ?";

            let row = sqlx::query_as::<_, StepStatusRow>(sql)
                .bind(run_id.to_string())
                .bind(item_index as i64)
                .bind(&step_id)
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to fetch step status for run '{run_id}', step '{step_id}'")
                })?;

            match row {
                Some(row) => Ok(Some(row.into_entry()?)),
                None => Ok(None),
            }
        }
        .boxed()
    }

    fn get_step_statuses(
        &self,
        run_id: Uuid,
        item_index: Option<usize>,
        since_seqno: Option<SequenceNumber>,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepStatusEntry>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let mut qb = QueryBuilder::<Sqlite>::new(
                "SELECT step_id, step_index, item_index, status, component, \
                 result_json, journal_seqno, updated_at \
                 FROM step_statuses WHERE run_id = ",
            );
            qb.push_bind(run_id.to_string());

            if let Some(idx) = item_index {
                qb.push(" AND item_index = ");
                qb.push_bind(idx as i64);
            }

            if let Some(since) = since_seqno {
                qb.push(" AND journal_seqno > ");
                qb.push_bind(u64_to_sql(since.value()));
            }

            qb.push(" ORDER BY step_index, item_index");

            let rows = qb
                .build_query_as::<StepStatusRow>()
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("SQL fetch_all failed for step_statuses (run_id={run_id})")
                })?;

            let mut entries = Vec::with_capacity(rows.len());
            for row in rows {
                entries.push(row.into_entry().attach_printable_lazy(|| {
                    format!("failed to convert step status row for run_id={run_id}")
                })?);
            }

            Ok(entries)
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

            let row = sqlx::query_as::<_, RunStatusRow>(sql)
                .bind(run_id.to_string())
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| format!("failed to fetch run status for '{run_id}'"))?;

            match row {
                Some(row) => {
                    if matches!(row.status.as_str(), "completed" | "failed" | "cancelled") {
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
                let row = sqlx::query_as::<_, RunStatusRow>(sql)
                    .bind(run_id.to_string())
                    .fetch_optional(&pool)
                    .await
                    .change_context(StateError::Internal)
                    .attach_printable_lazy(|| {
                        format!("failed to fetch run status for '{run_id}' in poll loop")
                    })?;

                match row {
                    Some(row) => {
                        if matches!(row.status.as_str(), "completed" | "failed" | "cancelled") {
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

    fn upsert_plugin_components(
        &self,
        plugin: &str,
        components: &[stepflow_core::component::ComponentInfo],
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let plugin = plugin.to_string();
        let components = components.to_vec();
        async move {
            let mut tx = pool
                .begin()
                .await
                .change_context(StateError::Internal)
                .attach_printable("failed to begin transaction for component upsert")?;

            // Upsert each component. Use INSERT ... ON CONFLICT to only bump
            // last_updated when data actually changes.
            for info in &components {
                let component_id = info.component.path().to_string();
                let path = &info.path;
                let description = info.description.as_deref();
                let input_schema_json = info
                    .input_schema
                    .as_ref()
                    .and_then(|s| serde_json::to_string(s).ok());
                let output_schema_json = info
                    .output_schema
                    .as_ref()
                    .and_then(|s| serde_json::to_string(s).ok());

                sqlx::query(
                    r#"
                    INSERT INTO component_registrations
                        (plugin, component_id, path, description, input_schema_json, output_schema_json, last_updated)
                    VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
                    ON CONFLICT (plugin, component_id) DO UPDATE SET
                        path = excluded.path,
                        description = excluded.description,
                        input_schema_json = excluded.input_schema_json,
                        output_schema_json = excluded.output_schema_json,
                        last_updated = CASE
                            WHEN component_registrations.path != excluded.path
                              OR component_registrations.description IS NOT excluded.description
                              OR component_registrations.input_schema_json IS NOT excluded.input_schema_json
                              OR component_registrations.output_schema_json IS NOT excluded.output_schema_json
                            THEN datetime('now')
                            ELSE component_registrations.last_updated
                        END
                    "#,
                )
                .bind(&plugin)
                .bind(&component_id)
                .bind(path)
                .bind(description)
                .bind(&input_schema_json)
                .bind(&output_schema_json)
                .execute(&mut *tx)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to upsert component '{component_id}' for plugin '{plugin}'")
                })?;
            }

            // Remove stale entries for this plugin that are no longer reported
            if components.is_empty() {
                sqlx::query("DELETE FROM component_registrations WHERE plugin = ?")
                    .bind(&plugin)
                    .execute(&mut *tx)
                    .await
                    .change_context(StateError::Internal)?;
            } else {
                let component_ids: Vec<String> = components
                    .iter()
                    .map(|c| c.component.path().to_string())
                    .collect();

                let mut qb: QueryBuilder<'_, Sqlite> = QueryBuilder::new(
                    "DELETE FROM component_registrations WHERE plugin = ",
                );
                qb.push_bind(&plugin);
                qb.push(" AND component_id NOT IN (");

                let mut separated = qb.separated(", ");
                for id in &component_ids {
                    separated.push_bind(id);
                }
                separated.push_unseparated(")");

                qb.build()
                    .execute(&mut *tx)
                    .await
                    .change_context(StateError::Internal)
                    .attach_printable_lazy(|| {
                        format!("failed to remove stale components for plugin '{plugin}'")
                    })?;
            }

            tx.commit()
                .await
                .change_context(StateError::Internal)
                .attach_printable("failed to commit component upsert transaction")?;

            Ok(())
        }
        .boxed()
    }

    fn get_registered_components(
        &self,
        plugin: Option<&str>,
    ) -> BoxFuture<
        '_,
        error_stack::Result<Vec<stepflow_state::StoredComponentRegistration>, StateError>,
    > {
        let pool = self.pool.clone();
        let plugin = plugin.map(|s| s.to_string());
        async move {
            let rows = match &plugin {
                Some(p) => {
                    sqlx::query_as::<_, ComponentRegistrationRow>(
                        "SELECT plugin, component_id, path, description, input_schema_json, output_schema_json, last_updated \
                         FROM component_registrations WHERE plugin = ? ORDER BY component_id",
                    )
                    .bind(p)
                    .fetch_all(&pool)
                    .await
                }
                None => {
                    sqlx::query_as::<_, ComponentRegistrationRow>(
                        "SELECT plugin, component_id, path, description, input_schema_json, output_schema_json, last_updated \
                         FROM component_registrations ORDER BY plugin, component_id",
                    )
                    .fetch_all(&pool)
                    .await
                }
            }
            .change_context(StateError::Internal)
            .attach_printable("failed to fetch component registrations")?;

            let results = rows
                .into_iter()
                .map(|row| {
                    let input_schema = row
                        .input_schema_json
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok());
                    let output_schema = row
                        .output_schema_json
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok());
                    let last_updated = chrono::NaiveDateTime::parse_from_str(
                        &row.last_updated,
                        "%Y-%m-%d %H:%M:%S",
                    )
                    .unwrap_or_default()
                    .and_utc();

                    stepflow_state::StoredComponentRegistration {
                        plugin: row.plugin,
                        info: stepflow_core::component::ComponentInfo {
                            component: stepflow_core::workflow::Component::from_string(
                                row.component_id,
                            ),
                            path: row.path,
                            description: row.description,
                            input_schema,
                            output_schema,
                        },
                        last_updated,
                    }
                })
                .collect();

            Ok(results)
        }
        .boxed()
    }

    fn component_max_last_updated(
        &self,
        plugin: &str,
    ) -> BoxFuture<'_, error_stack::Result<Option<chrono::DateTime<chrono::Utc>>, StateError>> {
        let pool = self.pool.clone();
        let plugin = plugin.to_string();
        async move {
            let row: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT MAX(last_updated) FROM component_registrations WHERE plugin = ?",
            )
            .bind(&plugin)
            .fetch_optional(&pool)
            .await
            .change_context(StateError::Internal)?;

            let result = row
                .and_then(|(ts,)| ts)
                .and_then(|ts| chrono::NaiveDateTime::parse_from_str(&ts, "%Y-%m-%d %H:%M:%S").ok())
                .map(|ndt| ndt.and_utc());

            Ok(result)
        }
        .boxed()
    }

    fn has_component_registrations(
        &self,
        plugin: &str,
    ) -> BoxFuture<'_, error_stack::Result<bool, StateError>> {
        let pool = self.pool.clone();
        let plugin = plugin.to_string();
        async move {
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM component_registrations WHERE plugin = ? LIMIT 1",
            )
            .bind(&plugin)
            .fetch_one(&pool)
            .await
            .change_context(StateError::Internal)?;

            Ok(row.0 > 0)
        }
        .boxed()
    }
}

impl ExecutionJournal for SqliteStateStore {
    fn initialize_journal(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        async move { migrations::run_journal_migrations(&pool).await }.boxed()
    }

    fn write(
        &self,
        root_run_id: Uuid,
        event: JournalEvent,
    ) -> BoxFuture<'_, error_stack::Result<SequenceNumber, StateError>> {
        let pool = self.pool.clone();

        async move {
            // Get the next sequence number for this root run's journal.
            // All events for an execution tree share the same journal keyed by root_run_id.
            let max_seq_sql =
                "SELECT COALESCE(MAX(sequence), -1) as max_seq FROM journal_entries WHERE root_run_id = ?";
            let row = sqlx::query_as::<_, MaxSeqRow>(max_seq_sql)
                .bind(root_run_id.to_string())
                .fetch_one(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to fetch max sequence for root_run_id '{root_run_id}'")
                })?;

            let max_seq: i64 = row.max_seq.unwrap_or(-1);
            let next_seq = SequenceNumber::new((max_seq + 1) as u64);

            // Extract a representative run_id for the SQL column (used for indexing/debugging).
            // For TasksStarted with multiple runs, we use the first run's ID.
            let run_id = match &event {
                JournalEvent::RootRunCreated { run_id, .. }
                | JournalEvent::StepsNeeded { run_id, .. }
                | JournalEvent::RunCompleted { run_id, .. }
                | JournalEvent::TaskCompleted { run_id, .. }
                | JournalEvent::StepsUnblocked { run_id, .. }
                | JournalEvent::ItemCompleted { run_id, .. } => *run_id,
                JournalEvent::TasksStarted { runs } => {
                    runs.first().map(|r| r.run_id).unwrap_or(root_run_id)
                }
                JournalEvent::SubRunCreated { run_id, .. } => *run_id,
            };

            let event_type = match &event {
                JournalEvent::RootRunCreated { .. } => "root_run_created",
                JournalEvent::StepsNeeded { .. } => "steps_needed",
                JournalEvent::RunCompleted { .. } => "run_completed",
                JournalEvent::TasksStarted { .. } => "tasks_started",
                JournalEvent::TaskCompleted { .. } => "task_completed",
                JournalEvent::StepsUnblocked { .. } => "steps_unblocked",
                JournalEvent::ItemCompleted { .. } => "item_completed",
                JournalEvent::SubRunCreated { .. } => "sub_run_created",
            };

            let event_data =
                serde_json::to_string(&event).change_context(StateError::Serialization)?;

            let timestamp = chrono::Utc::now().to_rfc3339();

            let insert_sql = r#"
                INSERT INTO journal_entries (run_id, sequence, root_run_id, timestamp, event_type, event_data)
                VALUES (?, ?, ?, ?, ?, ?)
            "#;

            sqlx::query(insert_sql)
                .bind(run_id.to_string())
                .bind(next_seq.value() as i64)
                .bind(root_run_id.to_string())
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

    fn stream_from(
        &self,
        root_run_id: Uuid,
        from_sequence: SequenceNumber,
    ) -> stepflow_state::JournalEventStream<'_> {
        let pool = self.pool.clone();
        const BATCH_SIZE: i64 = 1000;

        Box::pin(async_stream::stream! {
            let mut cursor = from_sequence.value() as i64;

            loop {
                let sql = r#"
                    SELECT sequence, run_id, timestamp, event_data
                    FROM journal_entries
                    WHERE root_run_id = ? AND sequence >= ?
                    ORDER BY sequence
                    LIMIT ?
                "#;

                let rows = match sqlx::query_as::<_, JournalEntryRow>(sql)
                    .bind(root_run_id.to_string())
                    .bind(cursor)
                    .bind(BATCH_SIZE)
                    .fetch_all(&pool)
                    .await
                {
                    Ok(rows) => rows,
                    Err(e) => {
                        yield Err(error_stack::report!(StateError::Internal).attach(e));
                        return;
                    }
                };

                if rows.is_empty() {
                    break;
                }

                let last_seq = rows.last().unwrap().sequence;

                for row in rows.iter() {
                    let run_id = match Uuid::parse_str(&row.run_id) {
                        Ok(id) => id,
                        Err(e) => {
                            yield Err(error_stack::report!(StateError::Serialization).attach(e));
                            return;
                        }
                    };
                    let timestamp = chrono::DateTime::parse_from_rfc3339(&row.timestamp)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|e| {
                            log::warn!(
                                "Failed to parse journal timestamp '{}': {e}", row.timestamp
                            );
                            chrono::Utc::now()
                        });

                    match serde_json::from_str::<JournalEvent>(&row.event_data) {
                        Ok(event) => yield Ok(JournalEntry {
                            run_id,
                            root_run_id,
                            sequence: SequenceNumber::new(row.sequence as u64),
                            timestamp,
                            event,
                        }),
                        Err(e) => {
                            yield Err(error_stack::report!(StateError::Serialization).attach(e));
                            return;
                        }
                    }
                }

                if rows.len() < BATCH_SIZE as usize {
                    break;
                }

                // Advance cursor past the last sequence seen, handling gaps.
                cursor = last_seq + 1;
            }
        })
    }

    fn latest_sequence(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<SequenceNumber>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "SELECT MAX(sequence) as max_seq FROM journal_entries WHERE root_run_id = ?";

            let row = sqlx::query_as::<_, MaxSeqRow>(sql)
                .bind(root_run_id.to_string())
                .fetch_one(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to fetch latest sequence for root_run_id '{root_run_id}'")
                })?;

            Ok(row.max_seq.map(|s| SequenceNumber::new(s as u64)))
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

            let rows = sqlx::query_as::<_, RootJournalRow>(sql)
                .fetch_all(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| "failed to fetch active root journals")?;

            let mut infos = Vec::with_capacity(rows.len());
            for row in rows {
                let root_run_id =
                    Uuid::parse_str(&row.root_run_id).change_context(StateError::Internal)?;

                infos.push(RootJournalInfo {
                    root_run_id,
                    latest_sequence: SequenceNumber::new(row.latest_sequence as u64),
                    entry_count: row.entry_count as u64,
                });
            }

            Ok(infos)
        }
        .boxed()
    }
}

impl CheckpointStore for SqliteStateStore {
    fn initialize_checkpoint_store(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        async move { migrations::run_checkpoint_migrations(&pool).await }.boxed()
    }

    fn put_checkpoint(
        &self,
        root_run_id: Uuid,
        sequence: SequenceNumber,
        data: bytes::Bytes,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = r#"
                INSERT OR REPLACE INTO checkpoints (root_run_id, sequence_number, data)
                VALUES (?, ?, ?)
            "#;

            sqlx::query(sql)
                .bind(root_run_id.to_string())
                .bind(sequence.value() as i64)
                .bind(data.as_ref())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }
        .boxed()
    }

    fn get_latest_checkpoint(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<StoredCheckpoint>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = r#"
                SELECT sequence_number, data
                FROM checkpoints
                WHERE root_run_id = ?
            "#;

            let row = sqlx::query_as::<_, CheckpointRow>(sql)
                .bind(root_run_id.to_string())
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to fetch checkpoint for root_run_id '{root_run_id}'")
                })?;

            Ok(row.map(|r| StoredCheckpoint {
                sequence: SequenceNumber::new(r.sequence_number as u64),
                data: r.data.into(),
            }))
        }
        .boxed()
    }

    fn delete_checkpoints(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = "DELETE FROM checkpoints WHERE root_run_id = ?";

            sqlx::query(sql)
                .bind(root_run_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u64_to_sql_roundtrip() {
        let values: Vec<u64> = vec![
            // u64 endpoints
            0,
            1,
            u64::MAX - 1,
            u64::MAX,
            // Near the i64 sign-bit boundary (1 << 63)
            (1u64 << 63) - 1, // i64::MAX as u64
            1u64 << 63,       // i64::MAX as u64 + 1
            (1u64 << 63) + 1,
            // Small values
            2,
            100,
            1000,
            // Powers of 2
            1u64 << 16,
            1u64 << 32,
            1u64 << 48,
        ];

        for v in values {
            let sql = u64_to_sql(v);
            let back = sql_to_u64(sql);
            assert_eq!(back, v, "roundtrip failed for u64 {v}");
        }
    }

    #[test]
    fn test_sql_to_u64_roundtrip() {
        let values: Vec<i64> = vec![
            // i64 endpoints
            i64::MIN,
            i64::MIN + 1,
            i64::MAX - 1,
            i64::MAX,
            // Near zero
            -1,
            0,
            1,
            // Other
            -100,
            100,
        ];

        for v in values {
            let u = sql_to_u64(v);
            let back = u64_to_sql(u);
            assert_eq!(back, v, "roundtrip failed for i64 {v}");
        }
    }

    #[test]
    fn test_u64_to_sql_preserves_ordering() {
        // Pairs where a < b in u64; verify u64_to_sql(a) < u64_to_sql(b) in i64.
        let pairs: Vec<(u64, u64)> = vec![
            (0, 1),
            (0, u64::MAX),
            (1, u64::MAX),
            // Across the sign-bit boundary
            ((1u64 << 63) - 1, 1u64 << 63),
            ((1u64 << 63), (1u64 << 63) + 1),
            // Small vs large
            (0, 1u64 << 63),
            (100, u64::MAX - 100),
            // Adjacent values at interesting points
            (i64::MAX as u64, i64::MAX as u64 + 1),
        ];

        for (a, b) in pairs {
            let sa = u64_to_sql(a);
            let sb = u64_to_sql(b);
            assert!(
                sa < sb,
                "ordering not preserved: u64_to_sql({a}) = {sa}, u64_to_sql({b}) = {sb}"
            );
        }
    }

    #[test]
    fn test_u64_to_sql_known_mappings() {
        // 0u64 should map to i64::MIN (smallest i64)
        assert_eq!(u64_to_sql(0), i64::MIN);
        // u64::MAX should map to i64::MAX (largest i64)
        assert_eq!(u64_to_sql(u64::MAX), i64::MAX);
        // The sign-bit boundary: 1<<63 maps to 0i64
        assert_eq!(u64_to_sql(1u64 << 63), 0i64);
        // Just below: (1<<63)-1 maps to -1i64
        assert_eq!(u64_to_sql((1u64 << 63) - 1), -1i64);
    }
}
