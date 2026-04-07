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

use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::Write as _;

use error_stack::{Result, ResultExt as _};
use futures::future::{BoxFuture, FutureExt as _};
use sqlx::any::AnyPoolOptions;
use sqlx::{Any, AnyPool, FromRow};
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

// =========================================================================
// SqlDialect
// =========================================================================

/// The SQL dialect detected from the database URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    Sqlite,
    Postgres,
}

impl SqlDialect {
    /// Detect dialect from a database URL.
    pub fn from_url(url: &str) -> Result<Self, StateError> {
        if url.starts_with("sqlite:") {
            return Ok(SqlDialect::Sqlite);
        }
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            return Ok(SqlDialect::Postgres);
        }
        Err(
            error_stack::report!(StateError::Connection).attach_printable(format!(
                "Unsupported database URL scheme (expected sqlite: or postgres://): {url}"
            )),
        )
    }

    /// Check that the required feature is compiled in for this dialect.
    ///
    /// Returns an error with a helpful message telling the user which Cargo
    /// feature to enable.
    pub fn check_feature_enabled(&self) -> Result<(), StateError> {
        match self {
            SqlDialect::Sqlite => {
                #[cfg(feature = "sqlite")]
                return Ok(());
                #[cfg(not(feature = "sqlite"))]
                return Err(
                    error_stack::report!(StateError::Connection).attach_printable(
                        "SQLite support was not compiled in. \
                     Rebuild with the `sqlite` feature enabled to use SQLite storage.",
                    ),
                );
            }
            SqlDialect::Postgres => {
                #[cfg(feature = "postgres")]
                return Ok(());
                #[cfg(not(feature = "postgres"))]
                return Err(
                    error_stack::report!(StateError::Connection).attach_printable(
                        "PostgreSQL support was not compiled in. \
                     Rebuild with the `postgres` feature enabled to use PostgreSQL storage.",
                    ),
                );
            }
        }
    }
}

// =========================================================================
// Placeholder rewriting
// =========================================================================

/// Rewrite `?` placeholders to `$1, $2, …` for PostgreSQL. Returns the
/// original string borrowed for SQLite (zero-cost).
fn rewrite_placeholders(sql: &str, dialect: SqlDialect) -> Cow<'_, str> {
    match dialect {
        SqlDialect::Sqlite => Cow::Borrowed(sql),
        SqlDialect::Postgres => {
            if !sql.contains('?') {
                return Cow::Borrowed(sql);
            }
            let mut result = String::with_capacity(sql.len() + 32);
            let mut idx = 0u32;
            for ch in sql.chars() {
                if ch == '?' {
                    idx += 1;
                    write!(result, "${idx}").unwrap();
                } else {
                    result.push(ch);
                }
            }
            Cow::Owned(result)
        }
    }
}

// =========================================================================
// Helpers
// =========================================================================

/// Map u64 → i64 preserving ordering (for SQL INTEGER/BIGINT columns).
///
/// XORs the sign bit so that `0u64 → i64::MIN` and `u64::MAX → i64::MAX`.
/// This is a bijection that preserves the relative ordering of any two u64
/// values in the i64 domain, so SQL `>=` / `ORDER BY` comparisons work.
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

/// Parse a datetime string stored by either SQLite or PostgreSQL.
///
/// Handles:
/// - RFC 3339 (`2025-12-24T19:26:14Z`)
/// - SQLite CURRENT_TIMESTAMP (`2025-12-24 19:26:14`)
/// - PostgreSQL implicit timestamptz→text (`2025-12-24 19:26:14+00`)
/// - PostgreSQL with fractional seconds (`2025-12-24 19:26:14.123456+00`)
fn parse_sql_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    // RFC 3339
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    // SQLite CURRENT_TIMESTAMP format
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(naive.and_utc());
    }
    // PostgreSQL with fractional seconds and timezone
    if let Ok(dt) = chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%#z") {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    // PostgreSQL without fractional seconds but with timezone
    if let Ok(dt) = chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%#z") {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    None
}

// =========================================================================
// FromRow structs — typed column extraction for all SQL queries.
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
            .and_then(parse_sql_datetime)
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
        let created_at = parse_sql_datetime(&self.created_at).ok_or_else(|| {
            error_stack::report!(StateError::Internal)
                .attach_printable(format!("unparseable created_at: {}", self.created_at))
        })?;
        let completed_at = self.completed_at.as_deref().and_then(parse_sql_datetime);
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
        let created_at = parse_sql_datetime(&self.created_at)
            .ok_or_else(|| error_stack::report!(StateError::Internal))?;
        let completed_at = self.completed_at.as_deref().and_then(parse_sql_datetime);
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
        let completed_at = self.completed_at.as_deref().and_then(parse_sql_datetime);

        Ok(ItemResult {
            item_index: self.item_index as usize,
            status: parse_execution_status(&self.status),
            result,
            completed_at,
        })
    }
}

// =========================================================================
// SqlStateStoreConfig
// =========================================================================

pub use stepflow_config::{PgStateStoreConfig, SqlStateStoreConfig, SqliteStateStoreConfig};

// =========================================================================
// SqlStateStore
// =========================================================================

/// Ensure sqlx's `Any` drivers are installed exactly once.
static INSTALL_DRIVERS: std::sync::Once = std::sync::Once::new();

/// SQL-backed MetadataStore, BlobStore, ExecutionJournal, and CheckpointStore.
///
/// Works with both SQLite and PostgreSQL via sqlx's `Any` driver. The dialect
/// is detected from the connection URL and controls which migration DDL is used
/// and how placeholders are formatted.
pub struct SqlStateStore {
    pub(crate) pool: AnyPool,
    pub(crate) dialect: SqlDialect,
    completion_notifier: RunCompletionNotifier,
}

/// Backward-compatible type alias.
pub type SqliteStateStore = SqlStateStore;

/// PostgreSQL alias.
pub type PgStateStore = SqlStateStore;

impl SqlStateStore {
    /// Create a new SqlStateStore with the given configuration.
    ///
    /// The dialect is detected from the URL prefix (`sqlite:` or `postgres://`).
    /// If `auto_migrate` is true, runs all migrations for the detected dialect.
    pub async fn new(config: SqlStateStoreConfig) -> Result<Self, StateError> {
        INSTALL_DRIVERS.call_once(sqlx::any::install_default_drivers);

        let dialect = SqlDialect::from_url(&config.database_url)?;
        dialect.check_feature_enabled()?;

        let pool = {
            // SQLite in-memory databases are per-connection. When using AnyPool
            // with `sqlite::memory:`, each pool connection would get a separate
            // empty database. Restrict to 1 connection so migrations and queries
            // share the same database.
            let max_conn = if config.database_url.contains(":memory:") {
                1
            } else {
                config.max_connections
            };

            let opts = AnyPoolOptions::new()
                .max_connections(max_conn)
                .after_connect(move |conn, _meta| {
                    Box::pin(async move {
                        if dialect == SqlDialect::Sqlite {
                            sqlx::query("PRAGMA journal_mode=WAL")
                                .execute(&mut *conn)
                                .await?;
                            sqlx::query("PRAGMA busy_timeout=5000")
                                .execute(&mut *conn)
                                .await?;
                            sqlx::query("PRAGMA temp_store=MEMORY")
                                .execute(&mut *conn)
                                .await?;
                        }
                        Ok(())
                    })
                });

            opts.connect(&config.database_url)
                .await
                .change_context(StateError::Connection)
                .attach_printable_lazy(|| format!("Database URL: {}", config.database_url))?
        };

        if config.auto_migrate {
            run_all_migrations(&pool, dialect).await?;
        }

        Ok(Self {
            pool,
            dialect,
            completion_notifier: RunCompletionNotifier::new(),
        })
    }

    /// Execute a raw SQL statement (e.g. DDL) against the pool.
    ///
    /// Useful for test setup such as creating schemas.
    pub async fn execute_raw(&self, sql: &str) -> Result<(), StateError> {
        sqlx::query::<Any>(sql)
            .execute(&self.pool)
            .await
            .change_context(StateError::Internal)
            .attach_printable_lazy(|| format!("failed to execute: {sql}"))?;
        Ok(())
    }

    /// Create SqlStateStore directly from a database URL.
    pub async fn from_url(database_url: &str) -> Result<Self, StateError> {
        Self::new(SqlStateStoreConfig::from_url(database_url)).await
    }

    /// Ensure sqlx Any drivers are installed. Called automatically by constructors,
    /// but useful for tests that create raw `AnyPool` instances directly.
    pub fn ensure_drivers() {
        INSTALL_DRIVERS.call_once(sqlx::any::install_default_drivers);
    }

    /// Create an in-memory SQLite database for testing.
    pub async fn in_memory() -> Result<Self, StateError> {
        Self::from_url("sqlite::memory:").await
    }

    /// Rewrite `?` placeholders to `$N` for PostgreSQL; no-op for SQLite.
    fn sql<'a>(&self, query: &'a str) -> Cow<'a, str> {
        rewrite_placeholders(query, self.dialect)
    }

    /// Format the current timestamp as a string for SQL binding.
    ///
    /// Used in UPDATE statements (e.g. `SET completed_at = ?`) instead of
    /// relying on `CURRENT_TIMESTAMP`, which returns different types across
    /// dialects. Binding an explicit string keeps all columns consistently
    /// decodable as `String` via the `Any` driver.
    fn now_str() -> String {
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }

    /// Internal helper for creating a run in the database.
    async fn create_run_sync(
        pool: &AnyPool,
        dialect: SqlDialect,
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

        let created_at_seqno: i64 = u64_to_sql(params.created_at_seqno.value());

        // Portable upsert: ON CONFLICT DO NOTHING works in both SQLite 3.24+ and PostgreSQL.
        let sql = rewrite_placeholders(
            "INSERT INTO runs (id, flow_id, flow_name, status, overrides_json, root_run_id, parent_run_id, orchestrator_id, created_at_seqno) \
             VALUES (?, ?, ?, 'running', ?, ?, ?, ?, ?) \
             ON CONFLICT DO NOTHING",
            dialect,
        );

        sqlx::query::<Any>(&sql)
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

        let item_sql = rewrite_placeholders(
            "INSERT INTO run_items (run_id, item_index, input_json, status) \
             VALUES (?, ?, ?, 'running') \
             ON CONFLICT DO NOTHING",
            dialect,
        );
        for (idx, input) in params.inputs.iter().enumerate() {
            let input_json =
                serde_json::to_string(input.as_ref()).change_context(StateError::Serialization)?;
            sqlx::query::<Any>(&item_sql)
                .bind(params.run_id.to_string())
                .bind(idx as i64)
                .bind(&input_json)
                .execute(pool)
                .await
                .change_context(StateError::Internal)?;
        }

        Ok(())
    }
}

// =========================================================================
// Migration dispatch
// =========================================================================

/// Dispatch macro for migration functions. Routes to the correct dialect module
/// based on the `SqlDialect`, with compile-time feature gating.
macro_rules! dispatch_migration {
    ($pool:expr, $dialect:expr, $method:ident) => {
        match $dialect {
            #[cfg(feature = "sqlite")]
            SqlDialect::Sqlite => crate::sqlite_migrations::$method($pool).await,
            #[cfg(feature = "postgres")]
            SqlDialect::Postgres => crate::pg_migrations::$method($pool).await,
            // Unreachable: check_feature_enabled() in new() catches this before
            // we ever get here. But the compiler needs exhaustive match arms when
            // a feature is disabled.
            #[allow(unreachable_patterns)]
            _ => Err(error_stack::report!(StateError::Connection)
                .attach_printable("SQL dialect not compiled in")),
        }
    };
}

async fn run_all_migrations(pool: &AnyPool, dialect: SqlDialect) -> Result<(), StateError> {
    dispatch_migration!(pool, dialect, run_all_migrations)
}

async fn run_blob_migrations(pool: &AnyPool, dialect: SqlDialect) -> Result<(), StateError> {
    dispatch_migration!(pool, dialect, run_blob_migrations)
}

async fn run_metadata_migrations(pool: &AnyPool, dialect: SqlDialect) -> Result<(), StateError> {
    dispatch_migration!(pool, dialect, run_metadata_migrations)
}

async fn run_journal_migrations(pool: &AnyPool, dialect: SqlDialect) -> Result<(), StateError> {
    dispatch_migration!(pool, dialect, run_journal_migrations)
}

async fn run_checkpoint_migrations(pool: &AnyPool, dialect: SqlDialect) -> Result<(), StateError> {
    dispatch_migration!(pool, dialect, run_checkpoint_migrations)
}

// =========================================================================
// BlobStore
// =========================================================================

impl BlobStore for SqlStateStore {
    fn initialize_blob_store(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let dialect = self.dialect;
        async move { run_blob_migrations(&pool, dialect).await }.boxed()
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

            let sql = self.sql(
                "INSERT INTO blobs (id, data, blob_type, filename) VALUES (?, ?, ?, ?) \
                 ON CONFLICT DO NOTHING",
            );

            sqlx::query::<Any>(&sql)
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
            let sql = self.sql("SELECT data, blob_type, filename FROM blobs WHERE id = ?");

            let row = sqlx::query_as::<Any, BlobRow>(&sql)
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

// =========================================================================
// MetadataStore
// =========================================================================

impl MetadataStore for SqlStateStore {
    fn initialize_metadata_store(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let dialect = self.dialect;
        async move { run_metadata_migrations(&pool, dialect).await }.boxed()
    }

    fn create_run(
        &self,
        params: stepflow_state::CreateRunParams,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let dialect = self.dialect;
        async move { Self::create_run_sync(&pool, dialect, params).await }.boxed()
    }

    fn get_run(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<RunSummary>, StateError>> {
        let pool = self.pool.clone();

        async move {
            let sql = self.sql(
                "SELECT flow_name, flow_id, status, created_at, completed_at, root_run_id, \
                 parent_run_id, orchestrator_id, created_at_seqno, finished_at_seqno \
                 FROM runs WHERE id = ?",
            );

            let row = sqlx::query_as::<Any, RunRow>(&sql)
                .bind(run_id.to_string())
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| format!("failed to fetch run '{run_id}'"))?;

            match row {
                Some(run_row) => {
                    let items_sql =
                        self.sql("SELECT status, COUNT(*) as cnt FROM run_items WHERE run_id = ? GROUP BY status");
                    let stat_rows = sqlx::query_as::<Any, RunStatRow>(&items_sql)
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
                }
                None => Ok(None),
            }
        }
        .boxed()
    }

    fn list_runs(
        &self,
        filters: &RunFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RunSummary>, StateError>> {
        let pool = self.pool.clone();
        let filters = filters.clone();

        async move {
            // Build dynamic WHERE clause with `?` placeholders. Rewritten to
            // `$N` for PostgreSQL by `self.sql()` at the end.
            let mut sql = String::from(
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

            // Bind values in order: String or i64, tracked as an enum.
            #[derive(Clone)]
            enum BindValue {
                Str(String),
                Int(i64),
            }
            let mut binds: Vec<BindValue> = Vec::new();

            if let Some(ref status) = filters.status {
                sql.push_str(" AND r.status = ?");
                binds.push(BindValue::Str(status.as_str().to_string()));
            }

            if let Some(ref flow_name) = filters.flow_name {
                sql.push_str(" AND r.flow_name = ?");
                binds.push(BindValue::Str(flow_name.clone()));
            }

            if let Some(root_run_id) = filters.root_run_id {
                sql.push_str(" AND r.root_run_id = ?");
                binds.push(BindValue::Str(root_run_id.to_string()));
            }

            if let Some(parent_run_id) = filters.parent_run_id {
                sql.push_str(" AND r.parent_run_id = ?");
                binds.push(BindValue::Str(parent_run_id.to_string()));
            }

            if filters.roots_only == Some(true) {
                sql.push_str(" AND r.parent_run_id IS NULL");
            }

            if let Some(ref orch_filter) = filters.orchestrator_id {
                match orch_filter {
                    Some(orch_id) => {
                        sql.push_str(" AND r.orchestrator_id = ?");
                        binds.push(BindValue::Str(orch_id.clone()));
                    }
                    None => {
                        sql.push_str(" AND r.orchestrator_id IS NULL");
                    }
                }
            }

            if let Some(offset_gte) = filters.created_after_seqno {
                sql.push_str(" AND r.created_at_seqno >= ?");
                binds.push(BindValue::Int(u64_to_sql(offset_gte)));
            }

            if let Some(seqno) = filters.not_finished_before_seqno {
                sql.push_str(" AND (r.finished_at_seqno IS NULL OR r.finished_at_seqno >= ?)");
                binds.push(BindValue::Int(u64_to_sql(seqno)));
            }

            sql.push_str(" ORDER BY r.created_at DESC");

            if let Some(limit) = filters.limit {
                sql.push_str(" LIMIT ?");
                binds.push(BindValue::Int(limit as i64));

                if let Some(offset) = filters.offset {
                    sql.push_str(" OFFSET ?");
                    binds.push(BindValue::Int(offset as i64));
                }
            }

            let sql = self.sql(&sql);

            // Build the query and bind all values in order.
            let mut query = sqlx::query_as::<Any, ListRunsRow>(&sql);
            for bind in &binds {
                match bind {
                    BindValue::Str(s) => query = query.bind(s.clone()),
                    BindValue::Int(i) => query = query.bind(*i),
                }
            }

            let rows = query
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
        finished_at_seqno: Option<SequenceNumber>,
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

            let now = Self::now_str();

            if let Some(seqno) = finished_at_seqno.filter(|_| is_terminal) {
                let seqno_sql = u64_to_sql(seqno.value());
                let sql = self.sql(
                    "UPDATE runs SET status = ?, completed_at = ?, finished_at_seqno = ? WHERE id = ?",
                );
                sqlx::query::<Any>(&sql)
                    .bind(status.as_str())
                    .bind(&now)
                    .bind(seqno_sql)
                    .bind(run_id.to_string())
                    .execute(&pool)
                    .await
                    .change_context(StateError::Internal)?;
            } else if is_terminal {
                let sql = self.sql("UPDATE runs SET status = ?, completed_at = ? WHERE id = ?");
                sqlx::query::<Any>(&sql)
                    .bind(status.as_str())
                    .bind(&now)
                    .bind(run_id.to_string())
                    .execute(&pool)
                    .await
                    .change_context(StateError::Internal)?;
            } else {
                let sql = self.sql("UPDATE runs SET status = ? WHERE id = ?");
                sqlx::query::<Any>(&sql)
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

            let now = Self::now_str();
            let sql = self.sql(
                "UPDATE run_items \
                 SET status = ?, result_json = ?, step_statuses_json = ?, completed_at = ? \
                 WHERE run_id = ? AND item_index = ?",
            );

            let result = sqlx::query::<Any>(&sql)
                .bind(status)
                .bind(&result_json)
                .bind(&step_statuses_json)
                .bind(&now)
                .bind(run_id.to_string())
                .bind(item_index as i64)
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            if result.rows_affected() == 0 {
                return Err(error_stack::report!(StateError::RunNotFound { run_id }));
            }

            let count_sql = self.sql(
                "SELECT \
                    (SELECT COUNT(*) FROM run_items WHERE run_id = ?) as total, \
                    (SELECT COUNT(*) FROM run_items WHERE run_id = ? AND status IN ('completed', 'failed')) as done, \
                    (SELECT COUNT(*) FROM run_items WHERE run_id = ? AND status = 'failed') as failed",
            );

            let counts = sqlx::query_as::<Any, ItemCountsRow>(&count_sql)
                .bind(run_id.to_string())
                .bind(run_id.to_string())
                .bind(run_id.to_string())
                .fetch_one(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to fetch item counts for run '{run_id}'")
                })?;

            if counts.done >= counts.total {
                let run_status = if counts.failed > 0 {
                    "failed"
                } else {
                    "completed"
                };
                let now = Self::now_str();
                let update_sql = self.sql("UPDATE runs SET status = ?, completed_at = ? WHERE id = ?");
                sqlx::query::<Any>(&update_sql)
                    .bind(run_status)
                    .bind(&now)
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

            let sql_template = format!(
                "SELECT item_index, status, result_json, completed_at FROM run_items WHERE run_id = ? {order_clause}",
            );
            let sql = self.sql(&sql_template);

            let rows = sqlx::query_as::<Any, ItemResultRow>(&sql)
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
            let sql = self.sql("UPDATE runs SET orchestrator_id = ? WHERE id = ?");
            let result = sqlx::query::<Any>(&sql)
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
                let sql = "UPDATE runs SET orchestrator_id = NULL \
                           WHERE orchestrator_id IS NOT NULL AND status = 'running'";
                let result = sqlx::query::<Any>(sql)
                    .execute(&pool)
                    .await
                    .change_context(StateError::Internal)?;
                return Ok(result.rows_affected() as usize);
            }

            // Build a parameterized IN clause with dialect-correct placeholders.
            let placeholders_str = (0..live_ids.len())
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            let sql_template = format!(
                "UPDATE runs SET orchestrator_id = NULL \
                 WHERE orchestrator_id IS NOT NULL \
                 AND orchestrator_id NOT IN ({placeholders_str}) \
                 AND status = 'running'",
            );
            let sql = self.sql(&sql_template);
            let mut query = sqlx::query::<Any>(&sql);
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
            let now = Self::now_str();

            let sql = self.sql(
                "INSERT INTO step_statuses (run_id, item_index, step_id, step_index, status, component, result_json, journal_seqno) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(run_id, item_index, step_id) DO UPDATE SET \
                     status = excluded.status, \
                     component = COALESCE(excluded.component, step_statuses.component), \
                     result_json = COALESCE(excluded.result_json, step_statuses.result_json), \
                     journal_seqno = excluded.journal_seqno, \
                     updated_at = ?",
            );

            sqlx::query::<Any>(&sql)
                .bind(run_id.to_string())
                .bind(item_index as i64)
                .bind(&step_id)
                .bind(step_index as i64)
                .bind(status_str)
                .bind(&component)
                .bind(&result_json)
                .bind(seqno_sql)
                .bind(&now)
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
            let sql = self.sql(
                "SELECT step_id, step_index, item_index, status, component, result_json, journal_seqno, updated_at \
                 FROM step_statuses WHERE run_id = ? AND item_index = ? AND step_id = ?",
            );

            let row = sqlx::query_as::<Any, StepStatusRow>(&sql)
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
            let mut sql = String::from(
                "SELECT step_id, step_index, item_index, status, component, \
                 result_json, journal_seqno, updated_at \
                 FROM step_statuses WHERE run_id = ?",
            );

            #[derive(Clone)]
            enum Bind {
                Str(String),
                Int(i64),
            }
            let mut binds = vec![Bind::Str(run_id.to_string())];

            if let Some(idx) = item_index {
                sql.push_str(" AND item_index = ?");
                binds.push(Bind::Int(idx as i64));
            }

            if let Some(since) = since_seqno {
                sql.push_str(" AND journal_seqno > ?");
                binds.push(Bind::Int(u64_to_sql(since.value())));
            }

            sql.push_str(" ORDER BY step_index, item_index");

            let sql = self.sql(&sql);
            let mut query = sqlx::query_as::<Any, StepStatusRow>(&sql);
            for bind in &binds {
                match bind {
                    Bind::Str(s) => query = query.bind(s.clone()),
                    Bind::Int(i) => query = query.bind(*i),
                }
            }

            let rows = query
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
            let sql = self.sql("SELECT status FROM runs WHERE id = ?");

            let receiver = self.completion_notifier.subscribe();

            let row = sqlx::query_as::<Any, RunStatusRow>(&sql)
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
                let row = sqlx::query_as::<Any, RunStatusRow>(&sql)
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

            let now = Self::now_str();

            // Upsert each component. Use INSERT ... ON CONFLICT to only bump
            // last_updated when data actually changes.
            let upsert_sql = self.sql(
                "INSERT INTO component_registrations \
                    (plugin, component_id, path, description, input_schema_json, output_schema_json, last_updated) \
                 VALUES (?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT (plugin, component_id) DO UPDATE SET \
                    path = excluded.path, \
                    description = excluded.description, \
                    input_schema_json = excluded.input_schema_json, \
                    output_schema_json = excluded.output_schema_json, \
                    last_updated = CASE \
                        WHEN component_registrations.path != excluded.path \
                          OR component_registrations.description IS NOT excluded.description \
                          OR component_registrations.input_schema_json IS NOT excluded.input_schema_json \
                          OR component_registrations.output_schema_json IS NOT excluded.output_schema_json \
                        THEN excluded.last_updated \
                        ELSE component_registrations.last_updated \
                    END",
            );

            for info in &components {
                let component_id = info.component.path().to_string();
                let path = &info.path;
                let description = info.description.as_deref();
                let input_schema_json = info
                    .input_schema
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .change_context(StateError::Serialization)
                    .attach_printable_lazy(|| {
                        format!(
                            "failed to serialize input schema for component '{component_id}'"
                        )
                    })?;
                let output_schema_json = info
                    .output_schema
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .change_context(StateError::Serialization)
                    .attach_printable_lazy(|| {
                        format!(
                            "failed to serialize output schema for component '{component_id}'"
                        )
                    })?;

                sqlx::query::<Any>(&upsert_sql)
                    .bind(&plugin)
                    .bind(&component_id)
                    .bind(path)
                    .bind(description)
                    .bind(&input_schema_json)
                    .bind(&output_schema_json)
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .change_context(StateError::Internal)
                    .attach_printable_lazy(|| {
                        format!(
                            "failed to upsert component '{component_id}' for plugin '{plugin}'"
                        )
                    })?;
            }

            // Remove stale entries for this plugin that are no longer reported
            if components.is_empty() {
                let sql = self.sql("DELETE FROM component_registrations WHERE plugin = ?");
                sqlx::query::<Any>(&sql)
                    .bind(&plugin)
                    .execute(&mut *tx)
                    .await
                    .change_context(StateError::Internal)?;
            } else {
                let component_ids: Vec<String> = components
                    .iter()
                    .map(|c| c.component.path().to_string())
                    .collect();

                let placeholders =
                    (0..component_ids.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
                let sql_template = format!(
                    "DELETE FROM component_registrations WHERE plugin = ? \
                     AND component_id NOT IN ({placeholders})"
                );
                let sql = self.sql(&sql_template);
                let mut query = sqlx::query::<Any>(&sql).bind(&plugin);
                for id in &component_ids {
                    query = query.bind(id);
                }
                query
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
                    let sql = self.sql(
                        "SELECT plugin, component_id, path, description, input_schema_json, output_schema_json, last_updated \
                         FROM component_registrations WHERE plugin = ? ORDER BY component_id",
                    );
                    sqlx::query_as::<Any, ComponentRegistrationRow>(&sql)
                        .bind(p)
                        .fetch_all(&pool)
                        .await
                }
                None => {
                    sqlx::query_as::<Any, ComponentRegistrationRow>(
                        "SELECT plugin, component_id, path, description, input_schema_json, output_schema_json, last_updated \
                         FROM component_registrations ORDER BY plugin, component_id",
                    )
                    .fetch_all(&pool)
                    .await
                }
            }
            .change_context(StateError::Internal)
            .attach_printable("failed to fetch component registrations")?;

            let mut results = Vec::with_capacity(rows.len());
            for row in rows {
                let input_schema = row
                    .input_schema_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .change_context(StateError::Serialization)
                    .attach_printable_lazy(|| {
                        format!(
                            "failed to deserialize input schema for component '{}'",
                            row.component_id
                        )
                    })?;
                let output_schema = row
                    .output_schema_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .change_context(StateError::Serialization)
                    .attach_printable_lazy(|| {
                        format!(
                            "failed to deserialize output schema for component '{}'",
                            row.component_id
                        )
                    })?;
                let last_updated = parse_sql_datetime(&row.last_updated).ok_or_else(|| {
                    error_stack::report!(StateError::Serialization).attach_printable(format!(
                        "unparseable last_updated for component '{}': {}",
                        row.component_id, row.last_updated
                    ))
                })?;

                results.push(stepflow_state::StoredComponentRegistration {
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
                });
            }

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
            let sql = self.sql(
                "SELECT MAX(last_updated) FROM component_registrations WHERE plugin = ?",
            );
            let row: Option<(Option<String>,)> = sqlx::query_as(&sql)
                .bind(&plugin)
                .fetch_optional(&pool)
                .await
                .change_context(StateError::Internal)?;

            let result = match row.and_then(|(ts,)| ts) {
                Some(ts) => Some(parse_sql_datetime(&ts).ok_or_else(|| {
                    error_stack::report!(StateError::Serialization).attach_printable(format!(
                        "unparseable component_registrations.last_updated for plugin '{plugin}': {ts}"
                    ))
                })?),
                None => None,
            };

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
            let sql =
                self.sql("SELECT COUNT(*) FROM component_registrations WHERE plugin = ? LIMIT 1");
            let row: (i64,) = sqlx::query_as(&sql)
                .bind(&plugin)
                .fetch_one(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(row.0 > 0)
        }
        .boxed()
    }
}

// =========================================================================
// ExecutionJournal
// =========================================================================

impl ExecutionJournal for SqlStateStore {
    fn initialize_journal(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let dialect = self.dialect;
        async move { run_journal_migrations(&pool, dialect).await }.boxed()
    }

    fn write(
        &self,
        root_run_id: Uuid,
        event: JournalEvent,
    ) -> BoxFuture<'_, error_stack::Result<SequenceNumber, StateError>> {
        let pool = self.pool.clone();

        async move {
            let max_seq_sql = self.sql(
                "SELECT COALESCE(MAX(sequence), -1) as max_seq FROM journal_entries WHERE root_run_id = ?",
            );
            let row = sqlx::query_as::<Any, MaxSeqRow>(&max_seq_sql)
                .bind(root_run_id.to_string())
                .fetch_one(&pool)
                .await
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("failed to fetch max sequence for root_run_id '{root_run_id}'")
                })?;

            let max_seq: i64 = row.max_seq.unwrap_or(-1);
            let next_seq = SequenceNumber::new((max_seq + 1) as u64);

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

            let insert_sql = self.sql(
                "INSERT INTO journal_entries (run_id, sequence, root_run_id, timestamp, event_type, event_data) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            );

            sqlx::query::<Any>(&insert_sql)
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
        let dialect = self.dialect;
        const BATCH_SIZE: i64 = 1000;

        Box::pin(async_stream::stream! {
            let mut cursor = from_sequence.value() as i64;

            loop {
                let sql = rewrite_placeholders(
                    "SELECT sequence, run_id, timestamp, event_data \
                     FROM journal_entries \
                     WHERE root_run_id = ? AND sequence >= ? \
                     ORDER BY sequence \
                     LIMIT ?",
                    dialect,
                );

                let rows = match sqlx::query_as::<Any, JournalEntryRow>(&sql)
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
            let sql = self
                .sql("SELECT MAX(sequence) as max_seq FROM journal_entries WHERE root_run_id = ?");

            let row = sqlx::query_as::<Any, MaxSeqRow>(&sql)
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
            let sql = r#"
                SELECT
                    root_run_id,
                    MAX(sequence) as latest_sequence,
                    COUNT(*) as entry_count
                FROM journal_entries
                GROUP BY root_run_id
            "#;

            let rows = sqlx::query_as::<Any, RootJournalRow>(sql)
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

// =========================================================================
// CheckpointStore
// =========================================================================

impl CheckpointStore for SqlStateStore {
    fn initialize_checkpoint_store(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();
        let dialect = self.dialect;
        async move { run_checkpoint_migrations(&pool, dialect).await }.boxed()
    }

    fn put_checkpoint(
        &self,
        root_run_id: Uuid,
        sequence: SequenceNumber,
        data: bytes::Bytes,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let pool = self.pool.clone();

        async move {
            let now = Self::now_str();
            let sql = self.sql(
                "INSERT INTO checkpoints (root_run_id, sequence_number, data, created_at) \
                 VALUES (?, ?, ?, ?) \
                 ON CONFLICT (root_run_id) DO UPDATE SET \
                     sequence_number = excluded.sequence_number, \
                     data = excluded.data, \
                     created_at = excluded.created_at",
            );

            sqlx::query::<Any>(&sql)
                .bind(root_run_id.to_string())
                .bind(sequence.value() as i64)
                .bind(data.as_ref())
                .bind(&now)
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
            let sql =
                self.sql("SELECT sequence_number, data FROM checkpoints WHERE root_run_id = ?");

            let row = sqlx::query_as::<Any, CheckpointRow>(&sql)
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
            let sql = self.sql("DELETE FROM checkpoints WHERE root_run_id = ?");

            sqlx::query::<Any>(&sql)
                .bind(root_run_id.to_string())
                .execute(&pool)
                .await
                .change_context(StateError::Internal)?;

            Ok(())
        }
        .boxed()
    }
}

// =========================================================================
// Unit tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u64_to_sql_roundtrip() {
        let values: Vec<u64> = vec![
            0,
            1,
            u64::MAX - 1,
            u64::MAX,
            (1u64 << 63) - 1,
            1u64 << 63,
            (1u64 << 63) + 1,
            2,
            100,
            1000,
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
            i64::MIN,
            i64::MIN + 1,
            i64::MAX - 1,
            i64::MAX,
            -1,
            0,
            1,
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
        let pairs: Vec<(u64, u64)> = vec![
            (0, 1),
            (0, u64::MAX),
            (1, u64::MAX),
            ((1u64 << 63) - 1, 1u64 << 63),
            ((1u64 << 63), (1u64 << 63) + 1),
            (0, 1u64 << 63),
            (100, u64::MAX - 100),
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
        assert_eq!(u64_to_sql(0), i64::MIN);
        assert_eq!(u64_to_sql(u64::MAX), i64::MAX);
        assert_eq!(u64_to_sql(1u64 << 63), 0i64);
        assert_eq!(u64_to_sql((1u64 << 63) - 1), -1i64);
    }

    #[test]
    fn test_rewrite_placeholders_sqlite() {
        let sql = "SELECT * FROM foo WHERE id = ? AND name = ?";
        let result = rewrite_placeholders(sql, SqlDialect::Sqlite);
        assert_eq!(result, sql);
    }

    #[test]
    fn test_rewrite_placeholders_postgres() {
        let sql = "SELECT * FROM foo WHERE id = ? AND name = ?";
        let result = rewrite_placeholders(sql, SqlDialect::Postgres);
        assert_eq!(result, "SELECT * FROM foo WHERE id = $1 AND name = $2");
    }

    #[test]
    fn test_rewrite_placeholders_no_placeholders() {
        let sql = "SELECT * FROM foo";
        let result = rewrite_placeholders(sql, SqlDialect::Postgres);
        // Should return borrowed reference (no allocation)
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, sql);
    }

    #[test]
    fn test_dialect_from_url() {
        assert_eq!(
            SqlDialect::from_url("sqlite::memory:").unwrap(),
            SqlDialect::Sqlite
        );
        assert_eq!(
            SqlDialect::from_url("sqlite:test.db").unwrap(),
            SqlDialect::Sqlite
        );
        assert_eq!(
            SqlDialect::from_url("postgres://user:pass@host/db").unwrap(),
            SqlDialect::Postgres
        );
        assert_eq!(
            SqlDialect::from_url("postgresql://user:pass@host/db").unwrap(),
            SqlDialect::Postgres
        );
        assert!(SqlDialect::from_url("mysql://host/db").is_err());
    }

    #[test]
    fn test_parse_sql_datetime() {
        // RFC 3339
        assert!(parse_sql_datetime("2025-12-24T19:26:14Z").is_some());
        // SQLite CURRENT_TIMESTAMP
        assert!(parse_sql_datetime("2025-12-24 19:26:14").is_some());
        // PostgreSQL with timezone
        assert!(parse_sql_datetime("2025-12-24 19:26:14+00").is_some());
        // PostgreSQL with fractional seconds
        assert!(parse_sql_datetime("2025-12-24 19:26:14.123456+00").is_some());
        // Invalid
        assert!(parse_sql_datetime("not-a-date").is_none());
    }
}
