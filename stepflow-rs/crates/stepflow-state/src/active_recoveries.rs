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
//! journal replay begins and removed after the recovered executor completes
//! its first dispatch cycle. The `OrchestratorService` checks this set when
//! a task_id is not found — if the task's run_id is being recovered, it
//! returns `UNAVAILABLE` instead of `NOT_FOUND` so the worker can retry.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashSet;
use uuid::Uuid;

/// Tracks run_ids currently being recovered.
///
/// Thread-safe and cheaply cloneable (wraps Arc internally).
///
/// Also tracks a `startup_recovery_pending` flag that is set before the gRPC
/// server starts accepting connections and cleared after the initial startup
/// recovery pass completes. While this flag is set, the `OrchestratorService`
/// returns `UNAVAILABLE` instead of `NOT_FOUND` for unknown tasks, preventing
/// workers from treating the response as terminal during the recovery window.
#[derive(Clone)]
pub struct ActiveRecoveries {
    recovering: Arc<DashSet<Uuid>>,
    startup_pending: Arc<AtomicBool>,
}

impl Default for ActiveRecoveries {
    fn default() -> Self {
        Self::new()
    }
}

impl ActiveRecoveries {
    /// Create a new empty tracker with no startup recovery pending.
    pub fn new() -> Self {
        Self {
            recovering: Arc::new(DashSet::new()),
            startup_pending: Arc::new(AtomicBool::new(false)),
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

    /// Set the startup recovery pending flag.
    ///
    /// When `true`, the orchestrator has not yet completed its initial startup
    /// recovery pass. gRPC services should return `UNAVAILABLE` for unknown
    /// tasks instead of `NOT_FOUND` during this window.
    pub fn set_startup_pending(&self, pending: bool) {
        self.startup_pending.store(pending, Ordering::Release);
    }

    /// Check if startup recovery is still pending.
    pub fn is_startup_pending(&self) -> bool {
        self.startup_pending.load(Ordering::Acquire)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_contains_remove() {
        let ar = ActiveRecoveries::new();
        let id = Uuid::now_v7();

        assert!(!ar.contains(&id));
        ar.insert(id);
        assert!(ar.contains(&id));
        ar.remove(&id);
        assert!(!ar.contains(&id));
    }

    #[test]
    fn test_startup_pending_defaults_to_false() {
        let ar = ActiveRecoveries::new();
        assert!(!ar.is_startup_pending());
    }

    #[test]
    fn test_startup_pending_set_and_clear() {
        let ar = ActiveRecoveries::new();

        ar.set_startup_pending(true);
        assert!(ar.is_startup_pending());

        ar.set_startup_pending(false);
        assert!(!ar.is_startup_pending());
    }

    #[test]
    fn test_clone_shares_state() {
        let ar = ActiveRecoveries::new();
        let ar2 = ar.clone();
        let id = Uuid::now_v7();

        ar.insert(id);
        assert!(ar2.contains(&id));

        ar.set_startup_pending(true);
        assert!(ar2.is_startup_pending());

        // Removal via the clone is visible to the original.
        ar2.remove(&id);
        assert!(!ar.contains(&id));

        ar2.set_startup_pending(false);
        assert!(!ar.is_startup_pending());
    }
}
