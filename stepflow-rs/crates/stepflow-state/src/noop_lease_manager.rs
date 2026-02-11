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

//! No-op lease manager for single-orchestrator deployments.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use error_stack::Result;
use futures::FutureExt as _;
use futures::future::BoxFuture;
use uuid::Uuid;

use crate::{
    DEFAULT_LEASE_TTL_SECS, LeaseError, LeaseInfo, LeaseManager, LeaseResult, OrchestratorId,
    OrchestratorInfo,
};

/// Internal lease record for tracking acquired leases.
#[derive(Debug, Clone)]
struct LeaseRecord {
    owner: OrchestratorId,
    acquired_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

/// A no-op lease manager that always grants leases.
///
/// This implementation is suitable for single-orchestrator deployments where
/// there's no need for distributed coordination. All lease requests succeed
/// immediately, and there are never any orphaned runs.
///
/// Leases are tracked in memory so that `get_lease` reflects acquisitions.
///
/// # Example
///
/// ```rust
/// use stepflow_state::{LeaseManager, NoOpLeaseManager, OrchestratorId};
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = NoOpLeaseManager::new();
/// let run_id = Uuid::now_v7();
/// let orch_id = OrchestratorId::new("my-orchestrator");
///
/// // Always succeeds
/// let result = manager.acquire_lease(run_id, orch_id.clone()).await?;
/// assert!(result.is_acquired());
///
/// // Lease is now observable via get_lease
/// let lease = manager.get_lease(run_id).await?.expect("lease should exist");
/// assert_eq!(lease.owner, orch_id);
/// # Ok(())
/// # }
/// ```
pub struct NoOpLeaseManager {
    /// Active leases tracked in memory.
    leases: DashMap<Uuid, LeaseRecord>,
    /// TTL used for computing `expires_at` on acquired leases.
    ttl: Duration,
}

impl std::fmt::Debug for NoOpLeaseManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoOpLeaseManager")
            .field("ttl", &self.ttl)
            .finish()
    }
}

impl Default for NoOpLeaseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NoOpLeaseManager {
    /// Create a new no-op lease manager with the default TTL.
    pub fn new() -> Self {
        Self {
            leases: DashMap::new(),
            ttl: Duration::from_secs(DEFAULT_LEASE_TTL_SECS),
        }
    }

    /// Create a new no-op lease manager with an explicit TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            leases: DashMap::new(),
            ttl,
        }
    }
}

impl LeaseManager for NoOpLeaseManager {
    fn acquire_lease(
        &self,
        run_id: Uuid,
        orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<LeaseResult, LeaseError>> {
        let now = Utc::now();
        let expires_at =
            now + chrono::Duration::from_std(self.ttl).unwrap_or(chrono::Duration::hours(1));

        // Check if the lease is already owned by a different orchestrator
        if let Some(existing) = self.leases.get(&run_id)
            && existing.owner != orchestrator_id
        {
            let result = LeaseResult::OwnedBy {
                owner: existing.owner.clone(),
                expires_at: existing.expires_at,
            };
            return async move { Ok(result) }.boxed();
        }

        // Track the lease
        self.leases.insert(
            run_id,
            LeaseRecord {
                owner: orchestrator_id,
                acquired_at: now,
                expires_at,
            },
        );

        async move { Ok(LeaseResult::Acquired { expires_at }) }.boxed()
    }

    fn release_lease(
        &self,
        run_id: Uuid,
        orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<(), LeaseError>> {
        // Verify ownership before releasing
        if let Some(existing) = self.leases.get(&run_id)
            && existing.owner != orchestrator_id
        {
            return async move { Err(error_stack::report!(LeaseError::NotOwner)) }.boxed();
        }

        // Remove the lease
        self.leases.remove(&run_id);
        async move { Ok(()) }.boxed()
    }

    fn release_all(
        &self,
        orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<(), LeaseError>> {
        self.leases
            .retain(|_, record| record.owner != orchestrator_id);
        async move { Ok(()) }.boxed()
    }

    fn heartbeat(
        &self,
        _orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<(), LeaseError>> {
        async move { Ok(()) }.boxed()
    }

    fn get_lease(&self, run_id: Uuid) -> BoxFuture<'_, Result<Option<LeaseInfo>, LeaseError>> {
        // Look up the lease in our tracking map
        let lease_info = self.leases.get(&run_id).map(|record| LeaseInfo {
            run_id,
            owner: record.owner.clone(),
            acquired_at: record.acquired_at,
            expires_at: record.expires_at,
        });
        async move { Ok(lease_info) }.boxed()
    }

    fn list_orchestrators(&self) -> BoxFuture<'_, Result<Vec<OrchestratorInfo>, LeaseError>> {
        // Collect unique orchestrator IDs from active leases
        let mut orchestrators: HashMap<OrchestratorId, usize> = HashMap::new();
        for entry in self.leases.iter() {
            *orchestrators.entry(entry.owner.clone()).or_insert(0) += 1;
        }

        let result: Vec<OrchestratorInfo> = orchestrators
            .into_iter()
            .map(|(id, active_runs)| OrchestratorInfo {
                id,
                last_heartbeat: Utc::now(),
                active_runs,
            })
            .collect();

        async move { Ok(result) }.boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_acquire_lease() {
        let manager = NoOpLeaseManager::new();
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");

        let result = manager
            .acquire_lease(run_id, orch_id)
            .await
            .unwrap();

        assert!(result.is_acquired());
        assert!(result.expires_at() > Utc::now());
    }

    #[tokio::test]
    async fn test_noop_release_lease() {
        let manager = NoOpLeaseManager::new();
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");

        // Should not error
        manager.release_lease(run_id, orch_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_noop_heartbeat() {
        let manager = NoOpLeaseManager::new();
        let orch_id = OrchestratorId::new("test-orch");

        // Should not error
        manager.heartbeat(orch_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_lease_returns_some_after_acquire() {
        let manager = NoOpLeaseManager::new();
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");

        // Before acquiring, get_lease should return None
        let lease = manager.get_lease(run_id).await.unwrap();
        assert!(lease.is_none(), "Lease should not exist before acquire");

        // Acquire the lease
        let result = manager
            .acquire_lease(run_id, orch_id.clone())
            .await
            .unwrap();
        assert!(result.is_acquired());

        // Now get_lease should return Some with the correct owner
        let lease = manager.get_lease(run_id).await.unwrap();
        assert!(lease.is_some(), "Lease should exist after acquire");
        let lease = lease.unwrap();
        assert_eq!(lease.owner, orch_id);
        assert_eq!(lease.run_id, run_id);
    }

    #[tokio::test]
    async fn test_get_lease_returns_none_after_release() {
        let manager = NoOpLeaseManager::new();
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");

        // Acquire the lease
        manager
            .acquire_lease(run_id, orch_id.clone())
            .await
            .unwrap();

        // Verify lease exists
        let lease = manager.get_lease(run_id).await.unwrap();
        assert!(lease.is_some());

        // Release the lease
        manager.release_lease(run_id, orch_id).await.unwrap();

        // Now get_lease should return None
        let lease = manager.get_lease(run_id).await.unwrap();
        assert!(lease.is_none(), "Lease should not exist after release");
    }

    #[tokio::test]
    async fn test_list_orchestrators_reflects_active_leases() {
        let manager = NoOpLeaseManager::new();
        let orch_id = OrchestratorId::new("test-orch");

        // Initially no orchestrators
        let orchestrators = manager.list_orchestrators().await.unwrap();
        assert!(orchestrators.is_empty());

        // Acquire a lease
        let run_id1 = Uuid::now_v7();
        manager
            .acquire_lease(run_id1, orch_id.clone())
            .await
            .unwrap();

        // Now should have one orchestrator with one active run
        let orchestrators = manager.list_orchestrators().await.unwrap();
        assert_eq!(orchestrators.len(), 1);
        assert_eq!(orchestrators[0].id, orch_id);
        assert_eq!(orchestrators[0].active_runs, 1);

        // Acquire another lease with same orchestrator
        let run_id2 = Uuid::now_v7();
        manager
            .acquire_lease(run_id2, orch_id.clone())
            .await
            .unwrap();

        // Should still have one orchestrator but with two active runs
        let orchestrators = manager.list_orchestrators().await.unwrap();
        assert_eq!(orchestrators.len(), 1);
        assert_eq!(orchestrators[0].active_runs, 2);
    }
}
