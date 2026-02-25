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

use std::collections::HashMap;
use std::sync::Arc;

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_state::{
    BlobStore, CheckpointStore, ExecutionJournal, FilesystemBlobStore, FilesystemBlobStoreConfig,
    InMemoryStateStore, InstrumentedBlobStore, MetadataStore,
};
use stepflow_state_sql::{SqliteStateStore, SqliteStateStoreConfig};

use crate::{ConfigError, Result};

/// A concrete store instance that can provide different store trait implementations.
///
/// This enum tracks the underlying store type and provides fallible conversions
/// to the specific store traits. Future store implementations may only support
/// a subset of traits (e.g., a blob-only store).
#[derive(Clone)]
enum ConcreteStore {
    InMemory(Arc<InMemoryStateStore>),
    Sqlite(Arc<SqliteStateStore>),
    Filesystem(Arc<FilesystemBlobStore>),
}

impl ConcreteStore {
    /// Try to use this store as a MetadataStore.
    ///
    /// Returns an error if this store type doesn't support metadata storage.
    fn as_metadata(&self) -> Result<Arc<dyn MetadataStore>> {
        match self {
            ConcreteStore::InMemory(s) => Ok(s.clone()),
            ConcreteStore::Sqlite(s) => Ok(s.clone()),
            ConcreteStore::Filesystem(_) => Err(error_stack::report!(ConfigError::Configuration))
                .attach_printable("Filesystem store only supports blob storage, not metadata"),
        }
    }

    /// Try to use this store as a BlobStore.
    ///
    /// Returns an error if this store type doesn't support blob storage.
    fn as_blob(&self) -> Result<Arc<dyn BlobStore>> {
        let inner: Arc<dyn BlobStore> = match self {
            ConcreteStore::InMemory(s) => s.clone(),
            ConcreteStore::Sqlite(s) => s.clone(),
            ConcreteStore::Filesystem(s) => s.clone(),
        };
        Ok(Arc::new(InstrumentedBlobStore::new(inner)))
    }

    /// Try to use this store as an ExecutionJournal.
    ///
    /// Returns an error if this store type doesn't support journaling.
    fn as_journal(&self) -> Result<Arc<dyn ExecutionJournal>> {
        match self {
            ConcreteStore::InMemory(s) => Ok(s.clone()),
            ConcreteStore::Sqlite(s) => Ok(s.clone()),
            ConcreteStore::Filesystem(_) => Err(error_stack::report!(ConfigError::Configuration))
                .attach_printable(
                    "Filesystem store only supports blob storage, not execution journal",
                ),
        }
    }

    /// Try to use this store as a CheckpointStore.
    ///
    /// Returns an error if this store type doesn't support checkpointing.
    fn as_checkpoint(&self) -> Result<Arc<dyn CheckpointStore>> {
        match self {
            ConcreteStore::InMemory(s) => Ok(s.clone()),
            ConcreteStore::Sqlite(s) => Ok(s.clone()),
            ConcreteStore::Filesystem(_) => Err(error_stack::report!(ConfigError::Configuration))
                .attach_printable(
                    "Filesystem store only supports blob storage, not checkpoint storage",
                ),
        }
    }
}

/// Create a ConcreteStore from a StoreConfig.
async fn create_concrete(config: &StoreConfig) -> Result<ConcreteStore> {
    match config {
        StoreConfig::InMemory => Ok(ConcreteStore::InMemory(Arc::new(InMemoryStateStore::new()))),
        StoreConfig::Sqlite(sqlite_config) => {
            let store = SqliteStateStore::new(sqlite_config.clone())
                .await
                .change_context(ConfigError::Configuration)?;
            Ok(ConcreteStore::Sqlite(Arc::new(store)))
        }
        StoreConfig::Filesystem(fs_config) => {
            let store = match &fs_config.directory {
                Some(dir) => FilesystemBlobStore::new(dir.into())
                    .await
                    .change_context(ConfigError::Configuration)?,
                None => FilesystemBlobStore::temp().change_context(ConfigError::Configuration)?,
            };
            Ok(ConcreteStore::Filesystem(Arc::new(store)))
        }
    }
}

/// Collection of stores created from storage configuration.
pub struct Stores {
    pub metadata_store: Arc<dyn MetadataStore>,
    pub blob_store: Arc<dyn BlobStore>,
    pub execution_journal: Arc<dyn ExecutionJournal>,
    pub checkpoint_store: Arc<dyn CheckpointStore>,
}

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
#[schemars(transform = stepflow_core::discriminator_schema::AddDiscriminator::new("type"))]
pub enum StoreConfig {
    /// In-memory storage (default, for testing and demos).
    ///
    /// **Supported stores**: metadata, blobs, journal
    ///
    /// Data is not persisted across restarts. Useful for development,
    /// testing, and demos where persistence is not required.
    #[default]
    #[schemars(title = "InMemoryStore")]
    InMemory,
    /// SQLite-based persistent storage.
    ///
    /// **Supported stores**: metadata, blobs, journal
    ///
    /// Provides durable storage with automatic schema migrations.
    /// Suitable for single-instance deployments and development.
    #[schemars(title = "SqliteStore")]
    Sqlite(SqliteStateStoreConfig),
    /// Filesystem-based blob storage.
    ///
    /// **Supported stores**: blobs only
    ///
    /// Stores blobs as JSON files in a directory. If no directory is specified,
    /// a temporary directory is created and cleaned up when the store is dropped.
    /// Suitable for local development and single-instance deployments.
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
///
/// When multiple stores have identical configurations, they will share
/// a single backend instance (smart deduplication).
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
    fn get_configs(&self) -> (StoreConfig, StoreConfig, StoreConfig) {
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

    /// Create MetadataStore, BlobStore, and ExecutionJournal from this configuration.
    ///
    /// When multiple stores have identical configurations, they share a single
    /// backend instance (smart deduplication based on full config equality).
    /// Each store's `initialize_*` method is called to set up only the tables
    /// needed for its role.
    pub async fn create_stores(&self) -> Result<Stores> {
        let (metadata_config, blobs_config, journal_config) = self.get_configs();

        // Deduplicate: create each unique config only once
        let mut cache: HashMap<StoreConfig, ConcreteStore> = HashMap::new();
        for config in [&metadata_config, &blobs_config, &journal_config] {
            if !cache.contains_key(config) {
                let store = create_concrete(config).await?;
                cache.insert(config.clone(), store);
            }
        }

        // Look up the concrete stores for each role
        let metadata_concrete = cache[&metadata_config].clone();
        let blobs_concrete = cache[&blobs_config].clone();
        let journal_concrete = cache[&journal_config].clone();

        // Convert to trait objects, validating each store supports the required trait
        let metadata_store = metadata_concrete
            .as_metadata()
            .attach_printable("metadata store configuration")?;
        let blob_store = blobs_concrete
            .as_blob()
            .attach_printable("blob store configuration")?;
        let execution_journal = journal_concrete
            .as_journal()
            .attach_printable("journal configuration")?;
        // Checkpoint store uses the same backend as the journal
        let checkpoint_store = journal_concrete
            .as_checkpoint()
            .attach_printable("checkpoint store configuration")?;

        // Initialize each store (creates only the tables needed for its role)
        metadata_store
            .initialize_metadata_store()
            .await
            .change_context(ConfigError::Configuration)
            .attach_printable("failed to initialize metadata store")?;
        blob_store
            .initialize_blob_store()
            .await
            .change_context(ConfigError::Configuration)
            .attach_printable("failed to initialize blob store")?;
        execution_journal
            .initialize_journal()
            .await
            .change_context(ConfigError::Configuration)
            .attach_printable("failed to initialize execution journal")?;
        checkpoint_store
            .initialize_checkpoint_store()
            .await
            .change_context(ConfigError::Configuration)
            .attach_printable("failed to initialize checkpoint store")?;

        Ok(Stores {
            metadata_store,
            blob_store,
            execution_journal,
            checkpoint_store,
        })
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
    fn test_expanded_form_all_same() {
        let yaml = r#"
metadata:
  type: sqlite
  databaseUrl: "sqlite:test.db"
blobs:
  type: sqlite
  databaseUrl: "sqlite:test.db"
journal:
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
                assert_eq!(blobs, Some(metadata.clone()));
                assert_eq!(journal, Some(metadata.clone()));
            }
            _ => panic!("Expected Expanded"),
        }
    }

    #[test]
    fn test_expanded_form_mixed() {
        let yaml = r#"
metadata:
  type: sqlite
  databaseUrl: "sqlite:test.db"
blobs:
  type: sqlite
  databaseUrl: "sqlite:test.db"
journal:
  type: inMemory
"#;
        let config: StorageConfig = serde_yaml_ng::from_str(yaml).unwrap();
        match config {
            StorageConfig::Expanded {
                metadata,
                blobs,
                journal,
            } => {
                assert!(matches!(metadata, StoreConfig::Sqlite(_)));
                assert!(matches!(blobs, Some(StoreConfig::Sqlite(_))));
                assert!(matches!(journal, Some(StoreConfig::InMemory)));
            }
            _ => panic!("Expected Expanded"),
        }
    }

    #[test]
    fn test_expanded_form_defaults() {
        // Only metadata specified, blobs and journal should default to None
        // (which means they'll use metadata's config at runtime)
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
    fn test_config_equality_for_dedup() {
        let config1 = StoreConfig::Sqlite(SqliteStateStoreConfig {
            database_url: "sqlite:test.db".to_string(),
            max_connections: 10,
            auto_migrate: true,
        });
        let config2 = StoreConfig::Sqlite(SqliteStateStoreConfig {
            database_url: "sqlite:test.db".to_string(),
            max_connections: 10,
            auto_migrate: true,
        });
        let config3 = StoreConfig::Sqlite(SqliteStateStoreConfig {
            database_url: "sqlite:other.db".to_string(),
            max_connections: 10,
            auto_migrate: true,
        });

        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }
}
