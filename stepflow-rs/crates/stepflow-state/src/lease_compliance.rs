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

//! Compliance test suite for `LeaseManager` implementations.
//!
//! This module provides a comprehensive set of tests that any `LeaseManager` implementation
//! must pass. Use these tests to verify your implementation conforms to the trait contract.
//!
//! # Usage
//!
//! In your implementation's test module:
//!
//! ```ignore
//! #[cfg(test)]
//! mod tests {
//!     use stepflow_state::lease_compliance::LeaseComplianceTests;
//!
//!     #[tokio::test]
//!     async fn compliance_acquire_lease() {
//!         let manager = MyLeaseManager::new().await.unwrap();
//!         LeaseComplianceTests::test_acquire_lease_succeeds(&manager).await;
//!     }
//!
//!     // ... or run all tests at once:
//!     #[tokio::test]
//!     async fn compliance_all() {
//!         let manager = MyLeaseManager::new().await.unwrap();
//!         LeaseComplianceTests::run_all(&manager).await;
//!     }
//! }
//! ```

use std::future::Future;
use std::time::Duration;

use uuid::Uuid;

use crate::{LeaseManager, OrchestratorId};

/// Compliance test suite for LeaseManager implementations.
///
/// Each test method validates a specific aspect of the LeaseManager contract.
/// Implementations should pass all tests to ensure correct behavior.
pub struct LeaseComplianceTests;

impl LeaseComplianceTests {
    /// Run all compliance tests against the given lease manager implementation.
    ///
    /// This is a convenience method that runs every test in the suite.
    /// Tests are run sequentially and will panic on the first failure.
    pub async fn run_all<L: LeaseManager>(manager: &L) {
        Self::test_acquire_lease_succeeds(manager).await;
        Self::test_get_lease_after_acquire(manager).await;
        Self::test_get_lease_returns_none_before_acquire(manager).await;
        Self::test_release_lease_removes_lease(manager).await;
        Self::test_heartbeat_succeeds(manager).await;
        Self::test_list_orchestrators_reflects_leases(manager).await;
        Self::test_acquire_returns_owned_by_when_taken(manager).await;
        Self::test_acquire_same_owner_succeeds(manager).await;
        Self::test_release_by_non_owner_fails(manager).await;
        Self::test_release_all_clears_leases(manager).await;
        Self::test_list_orchestrators_multiple_orchestrators(manager).await;
    }

    /// Run all compliance tests with a fresh manager for each test.
    ///
    /// This version creates a new manager instance for each test, ensuring complete
    /// isolation between tests. Use this when tests may interfere with each other
    /// due to shared state.
    ///
    /// # Example
    ///
    /// ```ignore
    /// LeaseComplianceTests::run_all_isolated(|| async {
    ///     MyLeaseManager::new().await.unwrap()
    /// }).await;
    /// ```
    pub async fn run_all_isolated<L, F, Fut>(factory: F)
    where
        L: LeaseManager,
        F: Fn() -> Fut,
        Fut: Future<Output = L>,
    {
        Self::test_acquire_lease_succeeds(&factory().await).await;
        Self::test_get_lease_after_acquire(&factory().await).await;
        Self::test_get_lease_returns_none_before_acquire(&factory().await).await;
        Self::test_release_lease_removes_lease(&factory().await).await;
        Self::test_heartbeat_succeeds(&factory().await).await;
        Self::test_list_orchestrators_reflects_leases(&factory().await).await;
        Self::test_acquire_returns_owned_by_when_taken(&factory().await).await;
        Self::test_acquire_same_owner_succeeds(&factory().await).await;
        Self::test_release_by_non_owner_fails(&factory().await).await;
        Self::test_release_all_clears_leases(&factory().await).await;
        Self::test_list_orchestrators_multiple_orchestrators(&factory().await).await;
    }

    // =========================================================================
    // acquire_lease() tests
    // =========================================================================

    /// Test that acquire_lease() succeeds for a new run.
    ///
    /// Contract: Acquiring a lease for a run that has no lease should succeed.
    pub async fn test_acquire_lease_succeeds<L: LeaseManager>(manager: &L) {
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");
        let ttl = Duration::from_secs(30);

        let result = manager
            .acquire_lease(run_id, orch_id, ttl)
            .await
            .expect("acquire_lease should succeed");

        assert!(
            result.is_acquired(),
            "Should successfully acquire lease for new run"
        );
        assert!(
            result.expires_at() > chrono::Utc::now(),
            "Expiry time should be in the future"
        );
    }

    // =========================================================================
    // get_lease() tests
    // =========================================================================

    /// Test that get_lease() returns the lease after acquire.
    ///
    /// Contract: After acquiring a lease, get_lease should return the lease info
    /// with the correct owner.
    pub async fn test_get_lease_after_acquire<L: LeaseManager>(manager: &L) {
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");
        let ttl = Duration::from_secs(30);

        // Acquire the lease
        let acquire_result = manager
            .acquire_lease(run_id, orch_id.clone(), ttl)
            .await
            .expect("acquire_lease should succeed");
        assert!(acquire_result.is_acquired());

        // Get the lease
        let lease = manager
            .get_lease(run_id)
            .await
            .expect("get_lease should succeed")
            .expect("Lease should exist after acquire");

        assert_eq!(lease.run_id, run_id, "Run ID should match");
        assert_eq!(lease.owner, orch_id, "Owner should match");
        assert!(
            lease.expires_at > chrono::Utc::now(),
            "Expiry should be in the future"
        );
    }

    /// Test that get_lease() returns None before any lease is acquired.
    ///
    /// Contract: get_lease for a run that was never leased should return None.
    pub async fn test_get_lease_returns_none_before_acquire<L: LeaseManager>(manager: &L) {
        let run_id = Uuid::now_v7();

        let lease = manager
            .get_lease(run_id)
            .await
            .expect("get_lease should succeed");

        assert!(
            lease.is_none(),
            "get_lease should return None for unleased run"
        );
    }

    // =========================================================================
    // release_lease() tests
    // =========================================================================

    /// Test that release_lease() removes the lease.
    ///
    /// Contract: After releasing a lease, get_lease should return None.
    pub async fn test_release_lease_removes_lease<L: LeaseManager>(manager: &L) {
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");
        let ttl = Duration::from_secs(30);

        // Acquire the lease
        manager
            .acquire_lease(run_id, orch_id.clone(), ttl)
            .await
            .expect("acquire_lease should succeed");

        // Verify lease exists
        let lease = manager.get_lease(run_id).await.expect("get_lease failed");
        assert!(lease.is_some(), "Lease should exist after acquire");

        // Release the lease
        manager
            .release_lease(run_id, orch_id)
            .await
            .expect("release_lease should succeed");

        // Verify lease is gone
        let lease = manager.get_lease(run_id).await.expect("get_lease failed");
        assert!(
            lease.is_none(),
            "get_lease should return None after release"
        );
    }

    // =========================================================================
    // heartbeat() tests
    // =========================================================================

    /// Test that heartbeat() succeeds.
    ///
    /// Contract: Heartbeat should succeed without error.
    pub async fn test_heartbeat_succeeds<L: LeaseManager>(manager: &L) {
        let orch_id = OrchestratorId::new("test-orch");

        let ttl = Duration::from_secs(30);

        // Heartbeat should succeed
        manager
            .heartbeat(orch_id, ttl)
            .await
            .expect("heartbeat should succeed");
    }

    // =========================================================================
    // list_orchestrators() tests
    // =========================================================================

    /// Test that list_orchestrators() reflects active leases.
    ///
    /// Contract: After acquiring leases, list_orchestrators should show the
    /// orchestrator with the correct count of active runs.
    pub async fn test_list_orchestrators_reflects_leases<L: LeaseManager>(manager: &L) {
        let orch_id = OrchestratorId::new("compliance-test-orch");
        let ttl = Duration::from_secs(30);

        // Acquire two leases
        let run1 = Uuid::now_v7();
        let run2 = Uuid::now_v7();

        manager
            .acquire_lease(run1, orch_id.clone(), ttl)
            .await
            .expect("acquire_lease should succeed");
        manager
            .acquire_lease(run2, orch_id.clone(), ttl)
            .await
            .expect("acquire_lease should succeed");

        // List orchestrators
        let orchestrators = manager
            .list_orchestrators()
            .await
            .expect("list_orchestrators should succeed");

        // Find our orchestrator
        let our_orch = orchestrators.iter().find(|o| o.id == orch_id);
        assert!(our_orch.is_some(), "Our orchestrator should be in the list");

        let our_orch = our_orch.unwrap();
        assert_eq!(
            our_orch.active_runs, 2,
            "Orchestrator should have 2 active runs"
        );

        // Release one lease
        manager
            .release_lease(run1, orch_id.clone())
            .await
            .expect("release_lease should succeed");

        // List orchestrators again
        let orchestrators = manager
            .list_orchestrators()
            .await
            .expect("list_orchestrators should succeed");

        let our_orch = orchestrators.iter().find(|o| o.id == orch_id);
        assert!(our_orch.is_some());
        assert_eq!(
            our_orch.unwrap().active_runs,
            1,
            "Orchestrator should have 1 active run after release"
        );
    }

    // =========================================================================
    // Contention tests
    // =========================================================================

    /// Test that acquire returns `OwnedBy` when another orchestrator holds the lease.
    ///
    /// Contract: If orchestrator A holds a lease, orchestrator B's acquire attempt
    /// should return `OwnedBy { owner: A }`.
    pub async fn test_acquire_returns_owned_by_when_taken<L: LeaseManager>(manager: &L) {
        let run_id = Uuid::now_v7();
        let orch_a = OrchestratorId::new("orch-a");
        let orch_b = OrchestratorId::new("orch-b");
        let ttl = Duration::from_secs(30);

        // Orchestrator A acquires the lease
        let result = manager
            .acquire_lease(run_id, orch_a.clone(), ttl)
            .await
            .expect("acquire_lease should succeed");
        assert!(result.is_acquired(), "Orch A should acquire the lease");

        // Orchestrator B tries to acquire the same lease
        let result = manager
            .acquire_lease(run_id, orch_b, ttl)
            .await
            .expect("acquire_lease should succeed (not error)");
        assert!(!result.is_acquired(), "Orch B should NOT acquire the lease");
        match result {
            crate::LeaseResult::OwnedBy { owner, .. } => {
                assert_eq!(owner, orch_a, "Should report orch-a as the owner");
            }
            other => panic!("Expected OwnedBy, got {:?}", other),
        }
    }

    /// Test that the same orchestrator can re-acquire its own lease (idempotent).
    ///
    /// Contract: If orchestrator A already holds a lease, acquiring again should succeed.
    pub async fn test_acquire_same_owner_succeeds<L: LeaseManager>(manager: &L) {
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");
        let ttl = Duration::from_secs(30);

        // First acquire
        let result = manager
            .acquire_lease(run_id, orch_id.clone(), ttl)
            .await
            .expect("acquire_lease should succeed");
        assert!(result.is_acquired());

        // Same orchestrator re-acquires
        let result = manager
            .acquire_lease(run_id, orch_id.clone(), ttl)
            .await
            .expect("re-acquire should succeed");
        assert!(
            result.is_acquired(),
            "Same owner should be able to re-acquire"
        );
    }

    /// Test that releasing a lease by a non-owner fails.
    ///
    /// Contract: If orchestrator A holds a lease, orchestrator B's release attempt
    /// should fail with an error.
    pub async fn test_release_by_non_owner_fails<L: LeaseManager>(manager: &L) {
        let run_id = Uuid::now_v7();
        let orch_a = OrchestratorId::new("orch-a");
        let orch_b = OrchestratorId::new("orch-b");
        let ttl = Duration::from_secs(30);

        // Orchestrator A acquires the lease
        manager
            .acquire_lease(run_id, orch_a, ttl)
            .await
            .expect("acquire_lease should succeed");

        // Orchestrator B tries to release
        let result = manager.release_lease(run_id, orch_b).await;
        assert!(result.is_err(), "Release by non-owner should fail");

        // Verify lease is still intact
        let lease = manager
            .get_lease(run_id)
            .await
            .expect("get_lease should succeed");
        assert!(
            lease.is_some(),
            "Lease should still exist after failed release"
        );
    }

    /// Test that list_orchestrators correctly reports multiple orchestrators
    /// with different run counts.
    ///
    /// Contract: When multiple orchestrators hold leases, list_orchestrators
    /// should return each with the correct active_runs count.
    pub async fn test_list_orchestrators_multiple_orchestrators<L: LeaseManager>(manager: &L) {
        let orch_a = OrchestratorId::new("multi-orch-a");
        let orch_b = OrchestratorId::new("multi-orch-b");
        let ttl = Duration::from_secs(30);

        // orch-a: heartbeat + 3 runs
        manager
            .heartbeat(orch_a.clone(), ttl)
            .await
            .expect("heartbeat should succeed");
        for _ in 0..3 {
            manager
                .acquire_lease(Uuid::now_v7(), orch_a.clone(), ttl)
                .await
                .expect("acquire should succeed");
        }

        // orch-b: heartbeat + 1 run
        manager
            .heartbeat(orch_b.clone(), ttl)
            .await
            .expect("heartbeat should succeed");
        manager
            .acquire_lease(Uuid::now_v7(), orch_b.clone(), ttl)
            .await
            .expect("acquire should succeed");

        let orchestrators = manager
            .list_orchestrators()
            .await
            .expect("list_orchestrators should succeed");

        let a_info = orchestrators
            .iter()
            .find(|o| o.id == orch_a)
            .expect("orch-a should be in the list");
        let b_info = orchestrators
            .iter()
            .find(|o| o.id == orch_b)
            .expect("orch-b should be in the list");

        assert_eq!(
            a_info.active_runs, 3,
            "orch-a should have 3 active runs"
        );
        assert_eq!(
            b_info.active_runs, 1,
            "orch-b should have 1 active run"
        );
    }

    /// Test that release_all removes all leases for an orchestrator.
    ///
    /// Contract: After release_all, no leases should remain for the orchestrator.
    pub async fn test_release_all_clears_leases<L: LeaseManager>(manager: &L) {
        let orch_id = OrchestratorId::new("test-orch");
        let ttl = Duration::from_secs(30);

        // Acquire two leases
        let run1 = Uuid::now_v7();
        let run2 = Uuid::now_v7();
        manager
            .acquire_lease(run1, orch_id.clone(), ttl)
            .await
            .expect("acquire should succeed");
        manager
            .acquire_lease(run2, orch_id.clone(), ttl)
            .await
            .expect("acquire should succeed");

        // Verify both leases exist
        assert!(manager.get_lease(run1).await.unwrap().is_some());
        assert!(manager.get_lease(run2).await.unwrap().is_some());

        // Release all
        manager
            .release_all(orch_id)
            .await
            .expect("release_all should succeed");

        // Verify both leases are gone
        assert!(
            manager.get_lease(run1).await.unwrap().is_none(),
            "Lease 1 should be removed after release_all"
        );
        assert!(
            manager.get_lease(run2).await.unwrap().is_none(),
            "Lease 2 should be removed after release_all"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoOpLeaseManager;

    #[tokio::test]
    async fn noop_lease_manager_compliance() {
        LeaseComplianceTests::run_all_isolated(|| async { NoOpLeaseManager::new() }).await;
    }
}
