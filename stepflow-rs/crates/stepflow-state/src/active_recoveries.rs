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

//! Tracks which run_ids are currently being recovered.
//!
//! During recovery (startup or orphan claiming), run_ids are inserted before
//! journal replay begins and removed after the executor is spawned. The
//! `OrchestratorService` checks this set when a task_id is not found — if the
//! task's run_id is being recovered, it returns `NOT_READY` instead of
//! `NOT_FOUND` so the worker can retry.

use std::sync::Arc;

use dashmap::DashSet;
use uuid::Uuid;

/// Tracks run_ids currently being recovered.
///
/// Thread-safe and cheaply cloneable (wraps Arc internally).
#[derive(Clone)]
pub struct ActiveRecoveries {
    recovering: Arc<DashSet<Uuid>>,
}

impl Default for ActiveRecoveries {
    fn default() -> Self {
        Self::new()
    }
}

impl ActiveRecoveries {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            recovering: Arc::new(DashSet::new()),
        }
    }

    /// Mark a run_id as being recovered.
    pub fn insert(&self, run_id: Uuid) {
        self.recovering.insert(run_id);
    }

    /// Remove a run_id from recovery tracking.
    pub fn remove(&self, run_id: &Uuid) {
        self.recovering.remove(run_id);
    }

    /// Check if a run_id is currently being recovered.
    pub fn contains(&self, run_id: &Uuid) -> bool {
        self.recovering.contains(run_id)
    }
}

/// Extension trait for accessing `ActiveRecoveries` from the environment.
pub trait ActiveRecoveriesExt {
    /// Get the active recoveries tracker.
    fn active_recoveries(&self) -> ActiveRecoveries;
}

impl ActiveRecoveriesExt for stepflow_core::StepflowEnvironment {
    fn active_recoveries(&self) -> ActiveRecoveries {
        self.get::<ActiveRecoveries>()
            .expect("ActiveRecoveries not set in environment")
    }
}
