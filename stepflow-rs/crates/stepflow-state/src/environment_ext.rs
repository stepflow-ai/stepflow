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

//! Extension traits for StateStore, ExecutionJournal, LeaseManager, and ActiveExecutions
//! access in StepflowEnvironment.

use std::sync::Arc;

use stepflow_core::StepflowEnvironment;

use crate::{ActiveExecutions, ExecutionJournal, LeaseManager, StateStore};

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
        assert!(retrieved.is_some());
        assert!(Arc::strong_count(retrieved.unwrap()) >= 1);
    }

    #[test]
    fn test_lease_manager_ext_returns_none_if_not_set() {
        let env = StepflowEnvironment::new();
        assert!(env.lease_manager().is_none());
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
