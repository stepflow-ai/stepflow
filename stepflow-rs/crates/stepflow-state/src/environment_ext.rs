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

//! Extension trait for StateStore access in StepflowEnvironment.

use std::sync::Arc;

use stepflow_core::StepflowEnvironment;

use crate::StateStore;

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
}
