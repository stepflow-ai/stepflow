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
use chrono::Utc;
use error_stack::{Result, ResultExt as _};
use futures::FutureExt as _;
use futures::future::BoxFuture;
use uuid::Uuid;

use crate::{
    ExecutionJournal, LeaseError, LeaseInfo, LeaseManager, LeaseResult, MetadataStore,
    OrchestratorId, OrchestratorInfo, OrphanedRun, RunRecoveryInfo,
};

/// A no-op lease manager that always grants leases.
///
/// This implementation is suitable for single-orchestrator deployments where
/// there's no need for distributed coordination. All lease requests succeed
/// immediately, and there are never any orphaned runs.
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
/// let result = manager.acquire_lease(run_id, orch_id, Duration::from_secs(30)).await?;
/// assert!(result.is_acquired());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default)]
pub struct NoOpLeaseManager {
    /// The orchestrator ID to use as the "owner" in responses.
    /// If None, uses the requesting orchestrator's ID.
    default_owner: Option<OrchestratorId>,
}

impl NoOpLeaseManager {
    /// Create a new no-op lease manager.
    pub fn new() -> Self {
        Self {
            default_owner: None,
        }
    }

    /// Create a no-op lease manager with a specific default owner.
    ///
    /// This can be useful for testing scenarios where you want consistent
    /// ownership information in responses.
    pub fn with_owner(owner: OrchestratorId) -> Self {
        Self {
            default_owner: Some(owner),
        }
    }
}

impl LeaseManager for NoOpLeaseManager {
    fn acquire_lease(
        &self,
        _run_id: Uuid,
        _orchestrator_id: OrchestratorId,
        ttl: Duration,
    ) -> BoxFuture<'_, Result<LeaseResult, LeaseError>> {
        let expires_at =
            Utc::now() + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::hours(1));
        async move { Ok(LeaseResult::Acquired { expires_at }) }.boxed()
    }

    fn renew_lease(
        &self,
        _run_id: Uuid,
        _orchestrator_id: OrchestratorId,
        ttl: Duration,
    ) -> BoxFuture<'_, Result<LeaseResult, LeaseError>> {
        let expires_at =
            Utc::now() + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::hours(1));
        async move { Ok(LeaseResult::Acquired { expires_at }) }.boxed()
    }

    fn release_lease(
        &self,
        _run_id: Uuid,
        _orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<(), LeaseError>> {
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
        let default_owner = self.default_owner.clone();
        async move {
            // Return a synthetic lease with the default owner if set
            if let Some(owner) = default_owner {
                Ok(Some(LeaseInfo {
                    run_id,
                    owner,
                    acquired_at: Utc::now(),
                    expires_at: Utc::now() + chrono::Duration::hours(1),
                }))
            } else {
                // No active lease in no-op mode
                Ok(None)
            }
        }
        .boxed()
    }

    fn list_orchestrators(&self) -> BoxFuture<'_, Result<Vec<OrchestratorInfo>, LeaseError>> {
        let default_owner = self.default_owner.clone();
        async move {
            // Return the default owner as the only orchestrator, if set
            if let Some(owner) = default_owner {
                Ok(vec![OrchestratorInfo {
                    id: owner,
                    last_heartbeat: Utc::now(),
                    active_runs: 0,
                }])
            } else {
                Ok(Vec::new())
            }
        }
        .boxed()
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
            let pending_runs = metadata_store
                .list_pending_runs(limit)
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

                    // For now, use an empty journal offset to replay from the beginning
                    // In the future, we could store and retrieve the offset from metadata
                    recovery_infos.push(RunRecoveryInfo {
                        run_id: summary.run_id,
                        root_run_id: summary.root_run_id,
                        parent_run_id: summary.parent_run_id,
                        flow_id: summary.flow_id,
                        inputs,
                        variables: HashMap::new(), // Variables aren't stored in RunDetails currently
                        journal_offset: Bytes::new(), // Replay from beginning
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
    async fn test_noop_with_owner() {
        let owner = OrchestratorId::new("default-owner");
        let manager = NoOpLeaseManager::with_owner(owner.clone());

        let run_id = Uuid::now_v7();
        let lease = manager.get_lease(run_id).await.unwrap();

        assert!(lease.is_some());
        let lease = lease.unwrap();
        assert_eq!(lease.owner, owner);

        let orchestrators = manager.list_orchestrators().await.unwrap();
        assert_eq!(orchestrators.len(), 1);
        assert_eq!(orchestrators[0].id, owner);
    }

    #[tokio::test]
    async fn test_noop_without_owner() {
        let manager = NoOpLeaseManager::new();

        let run_id = Uuid::now_v7();
        let lease = manager.get_lease(run_id).await.unwrap();
        assert!(lease.is_none());

        let orchestrators = manager.list_orchestrators().await.unwrap();
        assert!(orchestrators.is_empty());
    }
}
