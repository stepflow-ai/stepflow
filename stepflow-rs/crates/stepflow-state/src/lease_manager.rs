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

//! Lease management for distributed workflow orchestration.
//!
//! This module provides abstractions for managing run ownership in multi-orchestrator
//! deployments. Leases ensure that only one orchestrator executes a given run at a time.
//!
//! # Design Philosophy
//!
//! The `LeaseManager` trait is a pure coordination primitive. It handles ownership
//! enforcement (acquire, renew, release) and orchestrator liveness (heartbeats), but
//! does **not** query the metadata store or journal. This separation keeps the trait
//! implementable by distributed backends like etcd, where:
//!
//! - Leases have native TTLs and expired keys are automatically deleted
//! - A single etcd lease per orchestrator can efficiently release all run keys
//!   during graceful shutdown (`release_all`)
//! - Watch events on key deletions enable push-based orphan detection (`watch_orphans`)
//!
//! Recovery logic (identifying which runs need to be resumed) lives in the
//! `stepflow_execution::recovery` module, which uses the `MetadataStore` as the
//! source of truth for run status.
//!
//! # Key Concepts
//!
//! - **Lease**: A time-limited ownership claim on a run. The owner must periodically
//!   renew the lease to maintain ownership.
//! - **Orchestrator ID**: Unique identifier for an orchestrator instance.
//! - **Orphaned Run**: A run whose lease has expired without completion, indicating
//!   the owning orchestrator likely crashed.

use std::time::Duration;

use chrono::{DateTime, Utc};
use error_stack::Result;
use futures::FutureExt as _;
use futures::future::BoxFuture;
use uuid::Uuid;

use crate::{LeaseInfo, OrchestratorId, OrchestratorInfo};

/// Error type for lease operations.
#[derive(Debug, thiserror::Error)]
pub enum LeaseError {
    /// The lease is owned by another orchestrator.
    #[error("lease owned by {owner}")]
    OwnedByOther { owner: OrchestratorId },

    /// The lease has expired.
    #[error("lease expired")]
    Expired,

    /// The orchestrator does not own the lease.
    #[error("not the lease owner")]
    NotOwner,

    /// Connection to the lease backend failed.
    #[error("connection failed")]
    ConnectionFailed,

    /// Internal error in the lease manager.
    #[error("internal error")]
    Internal,
}

/// Trait for managing run ownership leases in distributed deployments.
///
/// Implementations must be thread-safe and support concurrent lease operations
/// across multiple runs.
pub trait LeaseManager: Send + Sync {
    /// Attempt to acquire a lease on a run.
    ///
    /// # Arguments
    /// * `run_id` - The run to acquire a lease for
    /// * `orchestrator_id` - The orchestrator requesting the lease
    /// * `ttl` - How long the lease should be valid
    ///
    /// # Returns
    /// * `LeaseResult::Acquired` if the lease was granted
    /// * `LeaseResult::OwnedBy` if another orchestrator owns the lease
    fn acquire_lease(
        &self,
        run_id: Uuid,
        orchestrator_id: OrchestratorId,
        ttl: Duration,
    ) -> BoxFuture<'_, Result<LeaseResult, LeaseError>>;

    /// Renew an existing lease.
    ///
    /// The orchestrator must already own the lease. This extends the expiration time.
    ///
    /// # Arguments
    /// * `run_id` - The run whose lease to renew
    /// * `orchestrator_id` - The orchestrator renewing (must be current owner)
    /// * `ttl` - New TTL from now
    ///
    /// # Returns
    /// * `LeaseResult::Acquired` with new expiration if successful
    /// * Error if the orchestrator doesn't own the lease
    fn renew_lease(
        &self,
        run_id: Uuid,
        orchestrator_id: OrchestratorId,
        ttl: Duration,
    ) -> BoxFuture<'_, Result<LeaseResult, LeaseError>>;

    /// Release a lease, allowing other orchestrators to claim the run.
    ///
    /// Should be called when a run completes (success or failure).
    ///
    /// # Arguments
    /// * `run_id` - The run whose lease to release
    /// * `orchestrator_id` - The orchestrator releasing (must be current owner)
    fn release_lease(
        &self,
        run_id: Uuid,
        orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<(), LeaseError>>;

    /// Send a heartbeat to indicate this orchestrator is still alive.
    ///
    /// This is used to track active orchestrators for load balancing and
    /// orphan detection. Orchestrators that stop sending heartbeats may
    /// have their runs redistributed.
    ///
    /// # Arguments
    /// * `orchestrator_id` - The orchestrator sending the heartbeat
    /// * `ttl` - How long the heartbeat should be valid before the orchestrator
    ///   is considered dead
    fn heartbeat(
        &self,
        orchestrator_id: OrchestratorId,
        ttl: Duration,
    ) -> BoxFuture<'_, Result<(), LeaseError>>;

    /// Release all leases held by this orchestrator.
    ///
    /// This is intended for graceful shutdown — the orchestrator releases all
    /// its run leases in a single operation so they can be immediately reclaimed
    /// by other orchestrators.
    ///
    /// The default implementation returns `Ok(())` (no-op). Backends that can
    /// efficiently release all leases (e.g., etcd lease revocation) should
    /// override this.
    fn release_all(
        &self,
        _orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<(), LeaseError>> {
        async { Ok(()) }.boxed()
    }

    /// Get the current lease holder for a run, if any.
    ///
    /// # Arguments
    /// * `run_id` - The run to check
    ///
    /// # Returns
    /// The current lease info, or None if no active lease exists
    fn get_lease(&self, run_id: Uuid) -> BoxFuture<'_, Result<Option<LeaseInfo>, LeaseError>>;

    /// List all currently active orchestrators.
    ///
    /// This is useful for load balancing decisions and determining
    /// how many orphans each orchestrator should claim.
    fn list_orchestrators(&self) -> BoxFuture<'_, Result<Vec<OrchestratorInfo>, LeaseError>>;

    /// Subscribe to orphaned run notifications (push-based).
    ///
    /// Returns a receiver that yields run IDs as they become orphaned (e.g., when
    /// their lease expires). This enables immediate reaction to orphans rather
    /// than polling.
    ///
    /// # Returns
    /// * `Some(receiver)` - For implementations that support push notifications (e.g., etcd watches)
    /// * `None` - For implementations that only support polling (caller should use periodic recovery)
    ///
    /// The default implementation returns `None`, indicating polling should be used.
    fn watch_orphans(&self) -> Option<tokio::sync::mpsc::UnboundedReceiver<Uuid>> {
        None
    }
}

/// Result of a lease acquisition or renewal attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseResult {
    /// The lease was successfully acquired or renewed.
    Acquired {
        /// When the lease expires (must be renewed before this time).
        expires_at: DateTime<Utc>,
    },
    /// The lease is owned by another orchestrator.
    OwnedBy {
        /// The current lease owner.
        owner: OrchestratorId,
        /// When the current lease expires.
        expires_at: DateTime<Utc>,
    },
}

impl LeaseResult {
    /// Returns true if the lease was acquired.
    pub fn is_acquired(&self) -> bool {
        matches!(self, LeaseResult::Acquired { .. })
    }

    /// Returns the expiration time of the lease.
    pub fn expires_at(&self) -> DateTime<Utc> {
        match self {
            LeaseResult::Acquired { expires_at } => *expires_at,
            LeaseResult::OwnedBy { expires_at, .. } => *expires_at,
        }
    }
}
