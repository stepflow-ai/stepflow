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
        Self::test_renew_lease_updates_expiry(manager).await;
        Self::test_heartbeat_succeeds(manager).await;
        Self::test_list_orchestrators_reflects_leases(manager).await;
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
        Self::test_renew_lease_updates_expiry(&factory().await).await;
        Self::test_heartbeat_succeeds(&factory().await).await;
        Self::test_list_orchestrators_reflects_leases(&factory().await).await;
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
    // renew_lease() tests
    // =========================================================================

    /// Test that renew_lease() extends the expiry time.
    ///
    /// Contract: After renewing a lease, the expiry should be extended.
    pub async fn test_renew_lease_updates_expiry<L: LeaseManager>(manager: &L) {
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");
        let short_ttl = Duration::from_secs(10);
        let long_ttl = Duration::from_secs(60);

        // Acquire with short TTL
        let acquire_result = manager
            .acquire_lease(run_id, orch_id.clone(), short_ttl)
            .await
            .expect("acquire_lease should succeed");
        let original_expiry = acquire_result.expires_at();

        // Renew with longer TTL
        let renew_result = manager
            .renew_lease(run_id, orch_id.clone(), long_ttl)
            .await
            .expect("renew_lease should succeed");

        assert!(renew_result.is_acquired(), "Renew should succeed");
        assert!(
            renew_result.expires_at() > original_expiry,
            "Renewed expiry should be later than original"
        );

        // Verify via get_lease
        let lease = manager
            .get_lease(run_id)
            .await
            .expect("get_lease should succeed")
            .expect("Lease should exist");

        assert!(
            lease.expires_at >= renew_result.expires_at(),
            "get_lease should show renewed expiry"
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

        // Heartbeat should succeed
        manager
            .heartbeat(orch_id)
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
        assert!(
            our_orch.is_some(),
            "Our orchestrator should be in the list"
        );

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
