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

use bit_set::BitSet;
use stepflow_core::{FlowResult, status::StepStatus as CoreStepStatus};
use stepflow_state::{StateError, StateStore};
use tokio::sync::RwLock;
use uuid::Uuid;

use stepflow_core::workflow::StepId;

/// Write-through cache for step results and status updates
#[derive(Debug, Clone)]
pub struct WriteCache {
    /// Internal data protected by Arc for cheap cloning
    inner: Arc<WriteCacheInner>,
}

#[derive(Debug)]
struct WriteCacheInner {
    /// Cached step results by step index (Vec index = step index)
    step_results: RwLock<Vec<Option<FlowResult>>>,
    /// Cached step statuses by step index (Vec index = step index)
    step_statuses: RwLock<Vec<Option<CoreStepStatus>>>,
}

impl WriteCache {
    pub fn new(step_count: usize) -> Self {
        Self {
            inner: Arc::new(WriteCacheInner {
                step_results: RwLock::new(vec![None; step_count]),
                step_statuses: RwLock::new(vec![None; step_count]),
            }),
        }
    }

    /// Cache a step result
    pub async fn cache_step_result(&self, step_index: usize, result: FlowResult) {
        let mut step_results = self.inner.step_results.write().await;
        debug_assert!(step_index < step_results.len(), "Step index out of bounds");
        step_results[step_index] = Some(result);
    }

    /// Cache step status updates
    pub async fn cache_step_statuses(&self, status: CoreStepStatus, step_indices: &BitSet) {
        let mut step_statuses = self.inner.step_statuses.write().await;
        for step_index in step_indices.iter() {
            debug_assert!(step_index < step_statuses.len(), "Step index out of bounds");
            step_statuses[step_index] = Some(status);
        }
    }

    /// Get cached step result if available
    pub async fn get_step_result(&self, step_id: &StepId) -> Option<FlowResult> {
        let step_results = self.inner.step_results.read().await;
        step_results.get(step_id.index).and_then(|opt| opt.clone())
    }

    /// Get step result with read-through to state store
    ///
    /// This method first checks the cache, and if not found, falls back to the state store.
    /// This encapsulates the read-through pattern within the cache itself.
    pub async fn get_step_result_with_fallback(
        &self,
        step_id: &StepId,
        run_id: Uuid,
        state_store: &Arc<dyn StateStore>,
    ) -> Result<FlowResult, error_stack::Report<StateError>> {
        // First check cache
        if let Some(cached_result) = self.get_step_result(step_id).await {
            return Ok(cached_result);
        }

        // Not in cache, fetch from state store
        state_store.get_step_result(run_id, step_id.index).await
    }
}
