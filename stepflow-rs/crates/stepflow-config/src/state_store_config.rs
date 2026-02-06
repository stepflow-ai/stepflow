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

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_state::{ExecutionJournal, InMemoryStateStore, StateStore};
use stepflow_state_sql::{SqliteStateStore, SqliteStateStoreConfig};

use crate::{ConfigError, Result};

#[derive(Serialize, Deserialize, Debug, Default, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StateStoreConfig {
    /// In-memory state store (default, for testing and demos)
    #[default]
    InMemory,
    /// SQLite-based persistent state store
    Sqlite(SqliteStateStoreConfig),
}

impl StateStoreConfig {
    /// Create a StateStore instance from this configuration
    pub async fn create_state_store(&self) -> Result<Arc<dyn StateStore>> {
        match self {
            StateStoreConfig::InMemory => Ok(Arc::new(InMemoryStateStore::new())),
            StateStoreConfig::Sqlite(config) => {
                let store = SqliteStateStore::new(config.clone())
                    .await
                    .change_context(ConfigError::Configuration)?;
                Ok(Arc::new(store))
            }
        }
    }

    /// Create both a StateStore and ExecutionJournal from this configuration.
    ///
    /// Both InMemoryStateStore and SqliteStateStore implement both traits,
    /// so this returns Arc references to the same underlying store.
    pub async fn create_stores(&self) -> Result<(Arc<dyn StateStore>, Arc<dyn ExecutionJournal>)> {
        match self {
            StateStoreConfig::InMemory => {
                let store = Arc::new(InMemoryStateStore::new());
                let journal: Arc<dyn ExecutionJournal> = store.clone();
                Ok((store, journal))
            }
            StateStoreConfig::Sqlite(config) => {
                let store = Arc::new(
                    SqliteStateStore::new(config.clone())
                        .await
                        .change_context(ConfigError::Configuration)?,
                );
                let journal: Arc<dyn ExecutionJournal> = store.clone();
                Ok((store, journal))
            }
        }
    }
}
