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

//! SQL-based StateStore implementation using sqlx.
//!
//! Provides persistent storage for blobs and execution state using SQLite and/or
//! PostgreSQL. The unified [`SqlStateStore`] works with both databases via sqlx's
//! `Any` driver; the dialect is detected from the connection URL.
//!
//! Enable features `sqlite` and/or `postgres` to include the respective drivers.

#[cfg(feature = "postgres")]
pub(crate) mod pg_migrations;
mod sql_state_store;
#[cfg(feature = "sqlite")]
pub(crate) mod sqlite_migrations;

pub use sql_state_store::{
    PgStateStore, PgStateStoreConfig, SqlDialect, SqlStateStore, SqlStateStoreConfig,
    SqliteStateStore, SqliteStateStoreConfig,
};

#[cfg(test)]
mod tests;
