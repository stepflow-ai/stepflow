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

//! Extension traits for StateStore, ExecutionJournal, and LeaseManager access in StepflowEnvironment.

use std::sync::Arc;

use stepflow_core::StepflowEnvironment;

use crate::{ExecutionJournal, LeaseManager, StateStore};

/// Extension trait providing StateStore access for StepflowEnvironment.
///
/// This trait allows crates that need state store access to import this
/// extension and call `env.state_store()` without requiring stepflow-core
/// to have any knowledge of the StateStore type.
///
/// # Example
///
/// ```ignore
/// use stepflow_state::StateStoreExt;
///
/// async fn store_blob(env: &StepflowEnvironment, data: BlobData) {
///     let blob_id = env.state_store().put_blob(&data).await.unwrap();
/// }
/// ```
pub trait StateStoreExt {
    /// Get a reference to the state store.
    ///
    /// # Panics
    ///
    /// Panics if state store was not set during environment construction.
    fn state_store(&self) -> &Arc<dyn StateStore>;
}

impl StateStoreExt for StepflowEnvironment {
    fn state_store(&self) -> &Arc<dyn StateStore> {
        self.get::<Arc<dyn StateStore>>()
            .expect("StateStore not set in environment")
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
    /// Returns `None` if no journal was configured (e.g., journalling disabled).
    fn execution_journal(&self) -> Option<&Arc<dyn ExecutionJournal>>;
}

impl ExecutionJournalExt for StepflowEnvironment {
    fn execution_journal(&self) -> Option<&Arc<dyn ExecutionJournal>> {
        self.get::<Arc<dyn ExecutionJournal>>()
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
///     if let Some(lease_manager) = env.lease_manager() {
///         lease_manager.acquire_lease(run_id, orchestrator_id, ttl).await.unwrap();
///     }
/// }
/// ```
pub trait LeaseManagerExt {
    /// Get a reference to the lease manager.
    ///
    /// Returns `None` if no lease manager was configured (e.g., single-orchestrator mode
    /// without distributed coordination).
    fn lease_manager(&self) -> Option<&Arc<dyn LeaseManager>>;
}

impl LeaseManagerExt for StepflowEnvironment {
    fn lease_manager(&self) -> Option<&Arc<dyn LeaseManager>> {
        self.get::<Arc<dyn LeaseManager>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStateStore;

    #[test]
    fn test_state_store_ext() {
        let mut env = StepflowEnvironment::new();
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        env.insert(store);

        // Use the extension trait
        let retrieved = env.state_store();
        // Just verify we can access it - the actual store functionality
        // is tested elsewhere
        assert!(Arc::strong_count(retrieved) >= 1);
    }

    #[test]
    #[should_panic(expected = "StateStore not set")]
    fn test_state_store_ext_panics_if_not_set() {
        let env = StepflowEnvironment::new();
        let _ = env.state_store();
    }

    #[test]
    fn test_execution_journal_ext() {
        let mut env = StepflowEnvironment::new();
        let store = Arc::new(InMemoryStateStore::new());
        let journal: Arc<dyn ExecutionJournal> = store;
        env.insert(journal);

        // Use the extension trait
        let retrieved = env.execution_journal();
        assert!(retrieved.is_some());
        assert!(Arc::strong_count(retrieved.unwrap()) >= 1);
    }

    #[test]
    fn test_execution_journal_ext_returns_none_if_not_set() {
        let env = StepflowEnvironment::new();
        assert!(env.execution_journal().is_none());
    }

    #[test]
    fn test_lease_manager_ext() {
        use crate::NoOpLeaseManager;

        let mut env = StepflowEnvironment::new();
        let lease_manager: Arc<dyn LeaseManager> = Arc::new(NoOpLeaseManager::new());
        env.insert(lease_manager);

        // Use the extension trait
        let retrieved = env.lease_manager();
        assert!(retrieved.is_some());
        assert!(Arc::strong_count(retrieved.unwrap()) >= 1);
    }

    #[test]
    fn test_lease_manager_ext_returns_none_if_not_set() {
        let env = StepflowEnvironment::new();
        assert!(env.lease_manager().is_none());
    }
}
