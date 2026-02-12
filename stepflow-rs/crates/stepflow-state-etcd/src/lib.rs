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

//! etcd-backed lease manager for distributed Stepflow orchestration.
//!
//! This crate implements the [`LeaseManager`] trait using etcd v3 for distributed
//! coordination. It uses a single etcd lease per orchestrator, which enables:
//!
//! - Efficient `release_all` via a single lease revocation
//! - Automatic cleanup on orchestrator crash (etcd lease TTL expiry)
//! - Push-based orphan detection via etcd watch events
//!
//! # Key Schema
//!
//! | Key | Value | etcd Lease |
//! |---|---|---|
//! | `{prefix}/runs/{run_id}` | `LeaseValue` JSON | Orchestrator's lease |
//! | `{prefix}/heartbeats/{orch_id}` | `HeartbeatValue` JSON | Orchestrator's lease |

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use error_stack::{Result, ResultExt as _};
use etcd_client::{Client, Compare, CompareOp, GetOptions, Txn, TxnOp};
use futures::FutureExt as _;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use stepflow_state::{
    LeaseError, LeaseInfo, LeaseManager, LeaseResult, OrchestratorId, OrchestratorInfo,
};
use uuid::Uuid;

/// Configuration for the etcd lease manager.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EtcdLeaseManagerConfig {
    /// etcd endpoints (e.g., `["http://localhost:2379"]`).
    pub endpoints: Vec<String>,

    /// Key prefix for all stepflow lease keys.
    #[serde(default = "default_key_prefix")]
    pub key_prefix: String,
}

fn default_key_prefix() -> String {
    "/stepflow/leases".to_string()
}

/// Value stored in etcd for each run lease key.
#[derive(Debug, Serialize, Deserialize)]
struct LeaseValue {
    owner: String,
    acquired_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

/// Value stored in etcd for orchestrator heartbeat keys.
#[derive(Debug, Serialize, Deserialize)]
struct HeartbeatValue {
    last_heartbeat: DateTime<Utc>,
}

/// etcd-backed lease manager for distributed Stepflow orchestration.
///
/// Uses a single etcd lease per orchestrator. All run keys and the heartbeat
/// key are attached to this lease, enabling efficient lifecycle management:
///
/// - `release_all` revokes the etcd lease, deleting all attached keys in one RPC
/// - On crash, the etcd lease expires and all keys are automatically cleaned up
/// - `watch_orphans` watches for key DELETE events caused by lease expiry
pub struct EtcdLeaseManager {
    /// The etcd client (supports cloning; shares an underlying connection pool).
    client: Client,

    /// Key prefix for all lease-related keys.
    key_prefix: String,

    /// TTL for the orchestrator's etcd lease.
    ///
    /// Set at construction time and used for all lease grants. Heartbeats
    /// keep the lease alive; if an orchestrator stops heartbeating, the
    /// lease expires after this duration.
    ttl: Duration,

    /// The etcd lease ID for this orchestrator instance.
    ///
    /// Lazily initialized on first `acquire_lease` or `heartbeat` call.
    /// Protected by a tokio RwLock to allow concurrent reads during
    /// normal operation while serializing initialization.
    orchestrator_lease: tokio::sync::RwLock<Option<OrchestratorLease>>,
}

/// Tracks the etcd lease and its keep-alive stream for an orchestrator.
struct OrchestratorLease {
    lease_id: i64,
    orchestrator_id: OrchestratorId,
    /// Long-lived keep-alive handle. Sending a keep-alive on this reuses
    /// the existing gRPC stream instead of opening a new one each time.
    keeper: etcd_client::LeaseKeeper,
}

impl EtcdLeaseManager {
    /// Connect to etcd and create a new lease manager.
    ///
    /// The `ttl` controls the etcd lease duration. Heartbeats must arrive
    /// before the TTL expires or the lease (and all attached keys) will be
    /// deleted by etcd, triggering orphan detection.
    pub async fn connect(
        config: &EtcdLeaseManagerConfig,
        ttl: Duration,
    ) -> Result<Self, LeaseError> {
        let client = Client::connect(&config.endpoints, None)
            .await
            .change_context(LeaseError::ConnectionFailed)?;

        Ok(Self {
            client,
            key_prefix: config.key_prefix.clone(),
            ttl,
            orchestrator_lease: tokio::sync::RwLock::new(None),
        })
    }

    /// Create from an existing etcd client (useful for testing).
    pub fn new(client: Client, key_prefix: String, ttl: Duration) -> Self {
        Self {
            client,
            key_prefix,
            ttl,
            orchestrator_lease: tokio::sync::RwLock::new(None),
        }
    }

    /// Get or create the etcd lease for the given orchestrator.
    ///
    /// The etcd lease TTL is set from `self.ttl` (configured at construction).
    /// Subsequent calls return the cached lease ID without re-granting (the
    /// underlying lease TTL is maintained by `heartbeat` keep-alives, not by
    /// this method).
    ///
    /// If a lease already exists for a different orchestrator, it is replaced
    /// in-memory without revoking the previous etcd lease. This shouldn't
    /// happen in normal operation since each `EtcdLeaseManager` instance is
    /// bound to one orchestrator.
    async fn ensure_lease(&self, orchestrator_id: &OrchestratorId) -> Result<i64, LeaseError> {
        // Fast path: lease already exists for this orchestrator
        {
            let guard = self.orchestrator_lease.read().await;
            if let Some(ref lease) = *guard
                && lease.orchestrator_id == *orchestrator_id
            {
                return Ok(lease.lease_id);
            }
        }

        // Slow path: need to create a new lease
        let mut guard = self.orchestrator_lease.write().await;

        // Double-check after acquiring write lock
        if let Some(ref lease) = *guard
            && lease.orchestrator_id == *orchestrator_id
        {
            return Ok(lease.lease_id);
        }

        let ttl_secs = self.ttl.as_secs().max(1) as i64;
        let resp = self
            .client
            .clone()
            .lease_grant(ttl_secs, None)
            .await
            .change_context(LeaseError::Internal)?;

        let lease_id = resp.id();

        // Establish a long-lived keep-alive stream for this lease.
        // The keeper is reused by heartbeat() to avoid per-call stream setup.
        let (keeper, _stream) = self
            .client
            .clone()
            .lease_keep_alive(lease_id)
            .await
            .change_context(LeaseError::Internal)?;

        *guard = Some(OrchestratorLease {
            lease_id,
            orchestrator_id: orchestrator_id.clone(),
            keeper,
        });

        Ok(lease_id)
    }

    /// Build the etcd key for a run lease.
    fn run_key(&self, run_id: Uuid) -> String {
        format!("{}/runs/{}", self.key_prefix, run_id)
    }

    /// Build the etcd key for an orchestrator heartbeat.
    fn heartbeat_key(&self, orchestrator_id: &OrchestratorId) -> String {
        format!("{}/heartbeats/{}", self.key_prefix, orchestrator_id)
    }
}

impl LeaseManager for EtcdLeaseManager {
    fn acquire_lease(
        &self,
        run_id: Uuid,
        orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<LeaseResult, LeaseError>> {
        async move {
            let lease_id = self.ensure_lease(&orchestrator_id).await?;
            let key = self.run_key(run_id);
            let now = Utc::now();
            let expires_at = now
                + chrono::Duration::from_std(self.ttl)
                    .map_err(|_| error_stack::report!(LeaseError::Internal))?;

            let value = LeaseValue {
                owner: orchestrator_id.as_str().to_string(),
                acquired_at: now,
                expires_at,
            };
            let value_bytes = serde_json::to_vec(&value).change_context(LeaseError::Internal)?;

            // Transaction: if key doesn't exist, create it; otherwise read existing
            let txn = Txn::new()
                .when([Compare::create_revision(
                    key.as_bytes(),
                    CompareOp::Equal,
                    0,
                )])
                .and_then([TxnOp::put(
                    key.as_bytes(),
                    value_bytes.clone(),
                    Some(etcd_client::PutOptions::new().with_lease(lease_id)),
                )])
                .or_else([TxnOp::get(key.as_bytes(), None)]);

            let resp = self
                .client
                .clone()
                .txn(txn)
                .await
                .change_context(LeaseError::Internal)?;

            if resp.succeeded() {
                // Key didn't exist — we created it successfully
                Ok(LeaseResult::Acquired { expires_at })
            } else {
                // Key already exists — check the owner
                let get_responses = resp.op_responses();
                let get_resp = get_responses
                    .first()
                    .and_then(|r| {
                        if let etcd_client::TxnOpResponse::Get(g) = r {
                            Some(g)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| error_stack::report!(LeaseError::Internal))?;

                let kv = get_resp
                    .kvs()
                    .first()
                    .ok_or_else(|| error_stack::report!(LeaseError::Internal))?;

                let existing: LeaseValue =
                    serde_json::from_slice(kv.value()).change_context(LeaseError::Internal)?;

                if existing.owner == orchestrator_id.as_str() {
                    // Same owner — overwrite with our lease attachment
                    self.client
                        .clone()
                        .put(
                            key.as_bytes(),
                            value_bytes,
                            Some(etcd_client::PutOptions::new().with_lease(lease_id)),
                        )
                        .await
                        .change_context(LeaseError::Internal)?;
                    Ok(LeaseResult::Acquired { expires_at })
                } else {
                    // Different owner — derive expires_at from the etcd lease TTL
                    // rather than the stored value, which may be stale if the
                    // owner's heartbeat has been extending the lease.
                    let actual_expires_at = {
                        let etcd_lease_id = kv.lease();
                        if etcd_lease_id != 0 {
                            self.client
                                .clone()
                                .lease_time_to_live(etcd_lease_id, None)
                                .await
                                .ok()
                                .and_then(|resp| {
                                    let ttl_secs = resp.ttl();
                                    if ttl_secs > 0 {
                                        Some(Utc::now() + chrono::Duration::seconds(ttl_secs))
                                    } else {
                                        None
                                    }
                                })
                        } else {
                            None
                        }
                    };
                    Ok(LeaseResult::OwnedBy {
                        owner: OrchestratorId::new(existing.owner),
                        expires_at: actual_expires_at.unwrap_or(existing.expires_at),
                    })
                }
            }
        }
        .boxed()
    }

    fn release_lease(
        &self,
        run_id: Uuid,
        orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<(), LeaseError>> {
        async move {
            let key = self.run_key(run_id);

            // Read to verify ownership
            let resp = self
                .client
                .clone()
                .get(key.as_bytes(), None)
                .await
                .change_context(LeaseError::Internal)?;

            if let Some(kv) = resp.kvs().first() {
                let existing: LeaseValue =
                    serde_json::from_slice(kv.value()).change_context(LeaseError::Internal)?;

                if existing.owner != orchestrator_id.as_str() {
                    return Err(error_stack::report!(LeaseError::NotOwner));
                }
            }

            // Delete the key (key may already be gone if lease expired)
            self.client
                .clone()
                .delete(key.as_bytes(), None)
                .await
                .change_context(LeaseError::Internal)?;

            Ok(())
        }
        .boxed()
    }

    fn release_all(
        &self,
        _orchestrator_id: OrchestratorId,
    ) -> BoxFuture<'_, Result<(), LeaseError>> {
        async move {
            let mut guard = self.orchestrator_lease.write().await;
            if let Some(lease) = guard.take() {
                // Revoking the etcd lease automatically deletes all attached keys
                // (run keys + heartbeat key) in a single RPC.
                self.client
                    .clone()
                    .lease_revoke(lease.lease_id)
                    .await
                    .change_context(LeaseError::Internal)?;
            }
            Ok(())
        }
        .boxed()
    }

    fn heartbeat(&self, orchestrator_id: OrchestratorId) -> BoxFuture<'_, Result<(), LeaseError>> {
        async move {
            // Ensure the lease exists (idempotent after first call)
            let _lease_id = self.ensure_lease(&orchestrator_id).await?;

            // Send keep-alive on the long-lived stream (reuses the gRPC stream
            // established in ensure_lease, avoiding per-call stream setup overhead).
            {
                let mut guard = self.orchestrator_lease.write().await;
                if let Some(ref mut lease) = *guard {
                    lease
                        .keeper
                        .keep_alive()
                        .await
                        .change_context(LeaseError::Internal)?;
                }
            }

            // Also update the heartbeat key with the current timestamp
            let lease_id = {
                let guard = self.orchestrator_lease.read().await;
                guard.as_ref().map(|l| l.lease_id).unwrap_or(0)
            };
            let key = self.heartbeat_key(&orchestrator_id);
            let value = HeartbeatValue {
                last_heartbeat: Utc::now(),
            };
            let value_bytes = serde_json::to_vec(&value).change_context(LeaseError::Internal)?;

            self.client
                .clone()
                .put(
                    key.as_bytes(),
                    value_bytes,
                    Some(etcd_client::PutOptions::new().with_lease(lease_id)),
                )
                .await
                .change_context(LeaseError::Internal)?;

            Ok(())
        }
        .boxed()
    }

    fn get_lease(&self, run_id: Uuid) -> BoxFuture<'_, Result<Option<LeaseInfo>, LeaseError>> {
        async move {
            let key = self.run_key(run_id);
            let resp = self
                .client
                .clone()
                .get(key.as_bytes(), None)
                .await
                .change_context(LeaseError::Internal)?;

            match resp.kvs().first() {
                Some(kv) => {
                    let value: LeaseValue =
                        serde_json::from_slice(kv.value()).change_context(LeaseError::Internal)?;
                    Ok(Some(LeaseInfo {
                        run_id,
                        owner: OrchestratorId::new(value.owner),
                        acquired_at: value.acquired_at,
                        expires_at: value.expires_at,
                    }))
                }
                None => Ok(None),
            }
        }
        .boxed()
    }

    fn list_orchestrators(&self) -> BoxFuture<'_, Result<Vec<OrchestratorInfo>, LeaseError>> {
        async move {
            let heartbeat_prefix = format!("{}/heartbeats/", self.key_prefix);
            let runs_prefix = format!("{}/runs/", self.key_prefix);

            // Get all heartbeat keys
            let heartbeat_resp = self
                .client
                .clone()
                .get(
                    heartbeat_prefix.as_bytes(),
                    Some(GetOptions::new().with_prefix()),
                )
                .await
                .change_context(LeaseError::Internal)?;

            let mut orchestrators: HashMap<String, OrchestratorInfo> = HashMap::new();

            for kv in heartbeat_resp.kvs() {
                let key_str = match std::str::from_utf8(kv.key()) {
                    Ok(s) => s,
                    Err(e) => {
                        log::warn!(
                            "Skipping heartbeat key with invalid UTF-8 ({} bytes): {e}",
                            kv.key().len()
                        );
                        continue;
                    }
                };
                if let Some(orch_id) = key_str.strip_prefix(&heartbeat_prefix) {
                    let value: HeartbeatValue =
                        serde_json::from_slice(kv.value()).change_context(LeaseError::Internal)?;
                    orchestrators.insert(
                        orch_id.to_string(),
                        OrchestratorInfo {
                            id: OrchestratorId::new(orch_id),
                            last_heartbeat: value.last_heartbeat,
                            active_runs: 0,
                        },
                    );
                }
            }

            // Get all run keys to count per-orchestrator runs
            let runs_resp = self
                .client
                .clone()
                .get(
                    runs_prefix.as_bytes(),
                    Some(GetOptions::new().with_prefix()),
                )
                .await
                .change_context(LeaseError::Internal)?;

            for kv in runs_resp.kvs() {
                let value: LeaseValue =
                    serde_json::from_slice(kv.value()).change_context(LeaseError::Internal)?;
                orchestrators
                    .entry(value.owner.clone())
                    .and_modify(|info| info.active_runs += 1)
                    .or_insert_with(|| OrchestratorInfo {
                        id: OrchestratorId::new(&value.owner),
                        last_heartbeat: Utc::now(),
                        active_runs: 1,
                    });
            }

            Ok(orchestrators.into_values().collect())
        }
        .boxed()
    }

    fn watch_orphans(&self) -> Option<tokio::sync::mpsc::UnboundedReceiver<Uuid>> {
        let runs_prefix = format!("{}/runs/", self.key_prefix);
        let mut client = self.client.clone();
        let key_prefix = self.key_prefix.clone();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            let watch_opts = etcd_client::WatchOptions::new().with_prefix();
            let Ok(mut stream) = client.watch(runs_prefix.as_bytes(), Some(watch_opts)).await
            else {
                log::error!("Failed to start etcd watch for orphan detection");
                return;
            };

            let prefix = format!("{}/runs/", key_prefix);
            while let Ok(Some(resp)) = stream.message().await {
                for event in resp.events() {
                    // Only care about DELETE events (key removed due to lease expiry)
                    if event.event_type() == etcd_client::EventType::Delete
                        && let Some(kv) = event.kv()
                        && let Some(run_id_str) = std::str::from_utf8(kv.key())
                            .ok()
                            .and_then(|k| k.strip_prefix(&prefix))
                        && let Ok(run_id) = Uuid::parse_str(run_id_str)
                        && tx.send(run_id).is_err()
                    {
                        // Receiver dropped, stop watching
                        return;
                    }
                }
            }
        });

        Some(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lease_value_serde() {
        let now = Utc::now();
        let value = LeaseValue {
            owner: "orch-1".to_string(),
            acquired_at: now,
            expires_at: now + chrono::Duration::seconds(30),
        };
        let bytes = serde_json::to_vec(&value).unwrap();
        let decoded: LeaseValue = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.owner, "orch-1");
    }

    #[test]
    fn test_heartbeat_value_serde() {
        let now = Utc::now();
        let value = HeartbeatValue {
            last_heartbeat: now,
        };
        let bytes = serde_json::to_vec(&value).unwrap();
        let decoded: HeartbeatValue = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.last_heartbeat, now);
    }

    #[test]
    fn test_key_format() {
        let prefix = "/stepflow/leases";
        let run_id = Uuid::nil();

        // run_key format: {prefix}/runs/{run_id}
        let run_key = format!("{}/runs/{}", prefix, run_id);
        assert_eq!(
            run_key,
            "/stepflow/leases/runs/00000000-0000-0000-0000-000000000000"
        );

        // heartbeat_key format: {prefix}/heartbeats/{orch_id}
        let heartbeat_key = format!("{}/heartbeats/{}", prefix, "orch-1");
        assert_eq!(heartbeat_key, "/stepflow/leases/heartbeats/orch-1");
    }

    #[test]
    fn test_parse_run_id_from_key() {
        let run_id = Uuid::now_v7();
        let prefix = "/stepflow/leases";

        // Valid run key
        let key = format!("{}/runs/{}", prefix, run_id);
        let runs_prefix = format!("{}/runs/", prefix);
        let parsed = key
            .strip_prefix(&runs_prefix)
            .and_then(|s| Uuid::parse_str(s).ok());
        assert_eq!(parsed, Some(run_id));

        // Non-matching key
        let parsed = "/other/key"
            .strip_prefix(&runs_prefix)
            .and_then(|s| Uuid::parse_str(s).ok());
        assert_eq!(parsed, None);
    }

    #[test]
    fn test_default_key_prefix() {
        assert_eq!(default_key_prefix(), "/stepflow/leases");
    }

    #[test]
    fn test_config_serde() {
        let json = r#"{"endpoints":["http://localhost:2379"]}"#;
        let config: EtcdLeaseManagerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.endpoints, vec!["http://localhost:2379"]);
        assert_eq!(config.key_prefix, "/stepflow/leases");
    }
}
