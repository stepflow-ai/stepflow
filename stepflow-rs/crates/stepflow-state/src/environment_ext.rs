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

//! Extension traits for MetadataStore, ExecutionJournal, LeaseManager, and ActiveExecutions
//! access in StepflowEnvironment.

use std::sync::Arc;

use stepflow_core::StepflowEnvironment;

use crate::{
    ActiveExecutions, BlobStore, ExecutionJournal, LeaseManager, MetadataStore, OrchestratorId,
};

/// Extension trait providing MetadataStore access for StepflowEnvironment.
///
/// This trait allows crates that need metadata store access to import this
/// extension and call `env.metadata_store()` without requiring stepflow-core
/// to have any knowledge of the MetadataStore type.
///
/// # Example
///
/// ```ignore
/// use stepflow_state::MetadataStoreExt;
///
/// async fn get_run(env: &StepflowEnvironment, run_id: Uuid) {
///     let run = env.metadata_store().get_run(run_id).await.unwrap();
/// }
/// ```
pub trait MetadataStoreExt {
    /// Get a reference to the metadata store.
    ///
    /// # Panics
    ///
    /// Panics if metadata store was not set during environment construction.
    fn metadata_store(&self) -> &Arc<dyn MetadataStore>;
}

impl MetadataStoreExt for StepflowEnvironment {
    fn metadata_store(&self) -> &Arc<dyn MetadataStore> {
        self.get::<Arc<dyn MetadataStore>>()
            .expect("MetadataStore not set in environment")
    }
}

/// Extension trait providing BlobStore access for StepflowEnvironment.
///
/// This trait allows crates that need blob storage access to import this
/// extension and call `env.blob_store()` without requiring stepflow-core
/// to have any knowledge of the BlobStore type.
///
/// # Example
///
/// ```ignore
/// use stepflow_state::BlobStoreExt;
///
/// async fn store_blob(env: &StepflowEnvironment, data: ValueRef) {
///     let blob_id = env.blob_store().put_blob(data, BlobType::Data).await.unwrap();
/// }
/// ```
pub trait BlobStoreExt {
    /// Get a reference to the blob store.
    ///
    /// # Panics
    ///
    /// Panics if blob store was not set during environment construction.
    fn blob_store(&self) -> &Arc<dyn BlobStore>;
}

impl BlobStoreExt for StepflowEnvironment {
    fn blob_store(&self) -> &Arc<dyn BlobStore> {
        self.get::<Arc<dyn BlobStore>>()
            .expect("BlobStore not set in environment")
    }
}

/// Extension trait providing ExecutionJournal access for StepflowEnvironment.
///
/// This trait allows crates that need journal access to import this
/// extension and call `env.execution_journal()` without requiring stepflow-core
/// to have any knowledge of the ExecutionJournal type.
///
/// # Example
///
/// ```ignore
/// use stepflow_state::ExecutionJournalExt;
///
/// async fn append_event(env: &StepflowEnvironment, entry: JournalEntry) {
///     env.execution_journal().append(entry).await.unwrap();
/// }
/// ```
pub trait ExecutionJournalExt {
    /// Get a reference to the execution journal.
    ///
    /// # Panics
    ///
    /// Panics if execution journal was not set during environment construction.
    fn execution_journal(&self) -> &Arc<dyn ExecutionJournal>;
}

impl ExecutionJournalExt for StepflowEnvironment {
    fn execution_journal(&self) -> &Arc<dyn ExecutionJournal> {
        self.get::<Arc<dyn ExecutionJournal>>()
            .expect("ExecutionJournal not set in environment")
    }
}

/// Extension trait providing LeaseManager access for StepflowEnvironment.
///
/// This trait allows crates that need lease management to import this
/// extension and call `env.lease_manager()` without requiring stepflow-core
/// to have any knowledge of the LeaseManager type.
///
/// # Example
///
/// ```ignore
/// use stepflow_state::LeaseManagerExt;
///
/// async fn acquire_lease(env: &StepflowEnvironment, run_id: Uuid) {
///     env.lease_manager().acquire_lease(run_id, orchestrator_id, ttl).await.unwrap();
/// }
/// ```
pub trait LeaseManagerExt {
    /// Get a reference to the lease manager.
    ///
    /// # Panics
    ///
    /// Panics if lease manager was not set during environment construction.
    fn lease_manager(&self) -> &Arc<dyn LeaseManager>;
}

impl LeaseManagerExt for StepflowEnvironment {
    fn lease_manager(&self) -> &Arc<dyn LeaseManager> {
        self.get::<Arc<dyn LeaseManager>>()
            .expect("LeaseManager not set in environment")
    }
}

/// Extension trait providing OrchestratorId access for StepflowEnvironment.
///
/// Unlike other extension traits, this returns `Option` rather than panicking
/// because the orchestrator ID is only set in multi-orchestrator deployments.
/// Single-orchestrator mode (CLI, tests) does not set it.
pub trait OrchestratorIdExt {
    /// Get a reference to the orchestrator ID, if set.
    fn orchestrator_id(&self) -> Option<&OrchestratorId>;
}

impl OrchestratorIdExt for StepflowEnvironment {
    fn orchestrator_id(&self) -> Option<&OrchestratorId> {
        self.get::<OrchestratorId>()
    }
}

/// Extension trait providing ActiveExecutions access for StepflowEnvironment.
///
/// This trait allows crates that need to track running executions to import this
/// extension and call `env.active_executions()` without requiring stepflow-core
/// to have any knowledge of the ActiveExecutions type.
///
/// # Example
///
/// ```ignore
/// use stepflow_state::ActiveExecutionsExt;
///
/// fn check_active_runs(env: &StepflowEnvironment) {
///     let active = env.active_executions();
///     println!("Currently running: {} executions", active.count());
/// }
/// ```
pub trait ActiveExecutionsExt {
    /// Get a reference to the active executions tracker.
    ///
    /// # Panics
    ///
    /// Panics if active executions was not set during environment construction.
    fn active_executions(&self) -> &ActiveExecutions;
}

impl ActiveExecutionsExt for StepflowEnvironment {
    fn active_executions(&self) -> &ActiveExecutions {
        self.get::<ActiveExecutions>()
            .expect("ActiveExecutions not set in environment")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStateStore;

    #[test]
    fn test_metadata_store_ext() {
        let mut env = StepflowEnvironment::new();
        let store: Arc<dyn MetadataStore> = Arc::new(InMemoryStateStore::new());
        env.insert(store);

        // Use the extension trait
        let retrieved = env.metadata_store();
        // Just verify we can access it - the actual store functionality
        // is tested elsewhere
        assert!(Arc::strong_count(retrieved) >= 1);
    }

    #[test]
    #[should_panic(expected = "MetadataStore not set")]
    fn test_metadata_store_ext_panics_if_not_set() {
        let env = StepflowEnvironment::new();
        let _ = env.metadata_store();
    }

    #[test]
    fn test_execution_journal_ext() {
        let mut env = StepflowEnvironment::new();
        let store = Arc::new(InMemoryStateStore::new());
        let journal: Arc<dyn ExecutionJournal> = store;
        env.insert(journal);

        // Use the extension trait
        let retrieved = env.execution_journal();
        assert!(Arc::strong_count(retrieved) >= 1);
    }

    #[test]
    #[should_panic(expected = "ExecutionJournal not set")]
    fn test_execution_journal_ext_panics_if_not_set() {
        let env = StepflowEnvironment::new();
        let _ = env.execution_journal();
    }

    #[test]
    fn test_lease_manager_ext() {
        use crate::NoOpLeaseManager;

        let mut env = StepflowEnvironment::new();
        let lease_manager: Arc<dyn LeaseManager> = Arc::new(NoOpLeaseManager::new());
        env.insert(lease_manager);

        // Use the extension trait
        let retrieved = env.lease_manager();
        assert!(Arc::strong_count(retrieved) >= 1);
    }

    #[test]
    #[should_panic(expected = "LeaseManager not set")]
    fn test_lease_manager_ext_panics_if_not_set() {
        let env = StepflowEnvironment::new();
        let _ = env.lease_manager();
    }

    #[test]
    fn test_active_executions_ext() {
        let mut env = StepflowEnvironment::new();
        let active = ActiveExecutions::new();
        env.insert(active);

        // Use the extension trait
        let retrieved = env.active_executions();
        assert!(retrieved.is_empty());
    }

    #[test]
    #[should_panic(expected = "ActiveExecutions not set")]
    fn test_active_executions_ext_panics_if_not_set() {
        let env = StepflowEnvironment::new();
        let _ = env.active_executions();
    }
}
