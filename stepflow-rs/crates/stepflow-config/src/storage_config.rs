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

use crate::{FilesystemBlobStoreConfig, SqlStateStoreConfig, SqliteStateStoreConfig};

/// Configuration for a single storage backend.
///
/// Each variant documents which store types it supports:
/// - **metadata**: Flow and run metadata storage
/// - **blobs**: Content-addressable blob storage
/// - **journal**: Execution journal for recovery
#[derive(
    Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, Hash, schemars::JsonSchema,
)]
#[serde(tag = "type", rename_all = "camelCase")]
#[schemars(transform = stepflow_flow::discriminator_schema::AddDiscriminator::new("type"))]
pub enum StoreConfig {
    /// In-memory storage (default, for testing and demos).
    #[default]
    #[schemars(title = "InMemoryStore")]
    InMemory,
    /// SQLite-based persistent storage.
    #[schemars(title = "SqliteStore")]
    Sqlite(SqliteStateStoreConfig),
    /// PostgreSQL-based persistent storage.
    #[schemars(title = "PostgresStore")]
    Postgres(SqlStateStoreConfig),
    /// Filesystem-based blob storage (blobs only).
    #[schemars(title = "FilesystemStore")]
    Filesystem(FilesystemBlobStoreConfig),
}

/// Storage configuration supporting both simple and expanded forms.
///
/// # Simple form (all stores share one backend)
/// ```yaml
/// storageConfig:
///   type: sqlite
///   databaseUrl: "sqlite:workflow_state.db"
/// ```
///
/// # Expanded form (individual configs per store)
/// ```yaml
/// storageConfig:
///   metadata:
///     type: sqlite
///     databaseUrl: "sqlite:workflow_state.db"
///   blobs:
///     type: sqlite
///     databaseUrl: "sqlite:workflow_state.db"
///   journal:
///     type: inMemory
/// ```
#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema)]
#[serde(untagged)]
pub enum StorageConfig {
    /// Expanded form: individual config per store
    #[schemars(title = "ExpandedStorageConfig")]
    Expanded {
        /// Configuration for the metadata store
        metadata: StoreConfig,
        /// Configuration for the blob store (defaults to metadata config if not specified)
        #[serde(default)]
        blobs: Option<StoreConfig>,
        /// Configuration for the execution journal (defaults to metadata config if not specified)
        #[serde(default)]
        journal: Option<StoreConfig>,
    },
    /// Simple form: all stores share one backend
    #[schemars(title = "SimpleStorageConfig")]
    Simple(StoreConfig),
}

impl Default for StorageConfig {
    fn default() -> Self {
        StorageConfig::Simple(StoreConfig::default())
    }
}

impl StorageConfig {
    /// Get the effective backend configs for metadata, blobs, and journal.
    pub fn get_configs(&self) -> (StoreConfig, StoreConfig, StoreConfig) {
        match self {
            StorageConfig::Simple(config) => (config.clone(), config.clone(), config.clone()),
            StorageConfig::Expanded {
                metadata,
                blobs,
                journal,
            } => {
                let blobs_config = blobs.clone().unwrap_or_else(|| metadata.clone());
                let journal_config = journal.clone().unwrap_or_else(|| metadata.clone());
                (metadata.clone(), blobs_config, journal_config)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_form_deserialization() {
        let yaml = r#"
type: inMemory
"#;
        let config: StorageConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(matches!(
            config,
            StorageConfig::Simple(StoreConfig::InMemory)
        ));
    }

    #[test]
    fn test_simple_form_sqlite() {
        let yaml = r#"
type: sqlite
databaseUrl: "sqlite:test.db"
"#;
        let config: StorageConfig = serde_yaml_ng::from_str(yaml).unwrap();
        match config {
            StorageConfig::Simple(StoreConfig::Sqlite(c)) => {
                assert_eq!(c.database_url, "sqlite:test.db");
            }
            _ => panic!("Expected Simple(Sqlite)"),
        }
    }

    #[test]
    fn test_expanded_form_defaults() {
        let yaml = r#"
metadata:
  type: sqlite
  databaseUrl: "sqlite:test.db"
"#;
        let config: StorageConfig = serde_yaml_ng::from_str(yaml).unwrap();
        match config {
            StorageConfig::Expanded {
                metadata,
                blobs,
                journal,
            } => {
                assert!(matches!(metadata, StoreConfig::Sqlite(_)));
                assert!(blobs.is_none());
                assert!(journal.is_none());
            }
            _ => panic!("Expected Expanded"),
        }
    }

    #[test]
    fn test_simple_form_postgres() {
        let yaml = r#"
type: postgres
databaseUrl: "postgres://user:pass@localhost/db"
"#;
        let config: StorageConfig = serde_yaml_ng::from_str(yaml).unwrap();
        match config {
            StorageConfig::Simple(StoreConfig::Postgres(c)) => {
                assert_eq!(c.database_url, "postgres://user:pass@localhost/db");
            }
            _ => panic!("Expected Simple(Postgres)"),
        }
    }
}
