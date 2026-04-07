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

use serde::{Deserialize, Serialize};

/// Configuration for SqlStateStore.
///
/// The dialect (SQLite vs PostgreSQL) is detected from the `database_url` prefix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SqlStateStoreConfig {
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

impl SqlStateStoreConfig {
    /// Create a config from a database URL, using default settings.
    pub fn from_url(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            max_connections: default_max_connections(),
            auto_migrate: default_auto_migrate(),
        }
    }
}

/// Backward-compatible type alias.
pub type SqliteStateStoreConfig = SqlStateStoreConfig;

/// PostgreSQL configuration (same struct, dialect detected from URL).
pub type PgStateStoreConfig = SqlStateStoreConfig;
