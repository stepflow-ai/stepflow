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
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use error_stack::{Result, ResultExt as _};
use futures::FutureExt as _;
use futures::future::BoxFuture;
use uuid::Uuid;

use stepflow_core::status::ExecutionStatus;
use stepflow_dtos::RunFilters;

use crate::{
    ExecutionJournal, LeaseError, LeaseInfo, LeaseManager, LeaseResult, MetadataStore,
    OrchestratorId, OrchestratorInfo, OrphanedRun, RunRecoveryInfo,
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
/// use std::time::Duration;
/// use uuid::Uuid;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = NoOpLeaseManager::new();
/// let run_id = Uuid::now_v7();
/// let orch_id = OrchestratorId::new("my-orchestrator");
///
/// // Always succeeds
/// let result = manager.acquire_lease(run_id, orch_id.clone(), Duration::from_secs(30)).await?;
/// assert!(result.is_acquired());
///
/// // Lease is now observable via get_lease
/// let lease = manager.get_lease(run_id).await?.expect("lease should exist");
/// assert_eq!(lease.owner, orch_id);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default)]
pub struct NoOpLeaseManager {
    /// Active leases tracked in memory.
    leases: DashMap<Uuid, LeaseRecord>,
}

impl NoOpLeaseManager {
    /// Create a new no-op lease manager.
    pub fn new() -> Self {
        Self {
            leases: DashMap::new(),
        }
    }
}

impl LeaseManager for NoOpLeaseManager {
    fn acquire_lease(
        &self,
        run_id: Uuid,
        orchestrator_id: OrchestratorId,
        ttl: Duration,
    ) -> BoxFuture<'_, Result<LeaseResult, LeaseError>> {
        let now = Utc::now();
        let expires_at =
            now + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::hours(1));

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

    fn renew_lease(
        &self,
        run_id: Uuid,
        orchestrator_id: OrchestratorId,
        ttl: Duration,
    ) -> BoxFuture<'_, Result<LeaseResult, LeaseError>> {
        let now = Utc::now();
        let expires_at =
            now + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::hours(1));

        // Update or insert the lease
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
        _orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<(), LeaseError>> {
        // Remove the lease
        self.leases.remove(&run_id);
        async move { Ok(()) }.boxed()
    }

    fn list_orphaned_runs(&self) -> BoxFuture<'_, Result<Vec<OrphanedRun>, LeaseError>> {
        // No-op manager never has orphaned runs
        async move { Ok(Vec::new()) }.boxed()
    }

    fn claim_orphaned_runs(
        &self,
        _orchestrator_id: OrchestratorId,
        _max_claims: usize,
        _ttl: Duration,
    ) -> BoxFuture<'_, Result<Vec<Uuid>, LeaseError>> {
        // No-op manager never has orphaned runs to claim
        async move { Ok(Vec::new()) }.boxed()
    }

    fn heartbeat(&self, _orchestrator_id: OrchestratorId) -> BoxFuture<'_, Result<(), LeaseError>> {
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

    fn claim_for_recovery(
        &self,
        _orchestrator_id: OrchestratorId,
        metadata_store: &Arc<dyn MetadataStore>,
        _journal: &Arc<dyn ExecutionJournal>,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<RunRecoveryInfo>, LeaseError>> {
        let metadata_store = Arc::clone(metadata_store);

        async move {
            // For single-orchestrator deployments, we recover all pending runs
            // from the metadata store. No lease coordination is needed.
            let filters = RunFilters {
                status: Some(ExecutionStatus::Running),
                limit: Some(limit),
                ..Default::default()
            };
            let pending_runs = metadata_store
                .list_runs(&filters)
                .await
                .change_context(LeaseError::Internal)?;

            let mut recovery_infos = Vec::with_capacity(pending_runs.len());

            for summary in pending_runs {
                // Get full run details for inputs and variables
                let details = metadata_store
                    .get_run(summary.run_id)
                    .await
                    .change_context(LeaseError::Internal)?;

                if let Some(details) = details {
                    // Extract inputs from item_details
                    let inputs = details
                        .item_details
                        .as_ref()
                        .map(|items| items.iter().map(|item| item.input.clone()).collect())
                        .unwrap_or_default();

                    // Note: inputs and variables here are placeholders. Recovery extracts
                    // authoritative values from the RunCreated journal event, which contains
                    // the exact inputs and variables used when the run was originally created.
                    // journal_offset is empty to replay from the beginning.
                    recovery_infos.push(RunRecoveryInfo {
                        run_id: summary.run_id,
                        root_run_id: summary.root_run_id,
                        parent_run_id: summary.parent_run_id,
                        flow_id: summary.flow_id,
                        inputs,
                        variables: HashMap::new(),
                        journal_offset: Bytes::new(),
                    });
                }
            }

            Ok(recovery_infos)
        }
        .boxed()
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
            .acquire_lease(run_id, orch_id, Duration::from_secs(30))
            .await
            .unwrap();

        assert!(result.is_acquired());
        assert!(result.expires_at() > Utc::now());
    }

    #[tokio::test]
    async fn test_noop_renew_lease() {
        let manager = NoOpLeaseManager::new();
        let run_id = Uuid::now_v7();
        let orch_id = OrchestratorId::new("test-orch");

        let result = manager
            .renew_lease(run_id, orch_id, Duration::from_secs(60))
            .await
            .unwrap();

        assert!(result.is_acquired());
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
    async fn test_noop_no_orphans() {
        let manager = NoOpLeaseManager::new();

        let orphans = manager.list_orphaned_runs().await.unwrap();
        assert!(orphans.is_empty());

        let claimed = manager
            .claim_orphaned_runs(OrchestratorId::new("test"), 10, Duration::from_secs(30))
            .await
            .unwrap();
        assert!(claimed.is_empty());
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
            .acquire_lease(run_id, orch_id.clone(), Duration::from_secs(30))
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
            .acquire_lease(run_id, orch_id.clone(), Duration::from_secs(30))
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
            .acquire_lease(run_id1, orch_id.clone(), Duration::from_secs(30))
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
            .acquire_lease(run_id2, orch_id.clone(), Duration::from_secs(30))
            .await
            .unwrap();

        // Should still have one orchestrator but with two active runs
        let orchestrators = manager.list_orchestrators().await.unwrap();
        assert_eq!(orchestrators.len(), 1);
        assert_eq!(orchestrators[0].active_runs, 2);
    }
}
