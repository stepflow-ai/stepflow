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

//! Types for lease management.

use std::collections::HashMap;
use std::fmt;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use stepflow_core::BlobId;
use stepflow_core::workflow::ValueRef;
use uuid::Uuid;

/// Unique identifier for an orchestrator instance.
///
/// Each orchestrator should have a unique ID that persists across restarts
/// (typically generated once and stored in configuration or derived from
/// the hostname/pod name).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrchestratorId(String);

impl OrchestratorId {
    /// Create a new orchestrator ID from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a random orchestrator ID.
    ///
    /// Useful for testing or when a persistent ID isn't required.
    pub fn random() -> Self {
        Self(Uuid::now_v7().to_string())
    }

    /// Get the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Take the ID as an owned string.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for OrchestratorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for OrchestratorId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for OrchestratorId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// Current lease information for a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseInfo {
    /// The run ID.
    pub run_id: Uuid,

    /// The current lease owner.
    pub owner: OrchestratorId,

    /// When the lease was acquired.
    pub acquired_at: DateTime<Utc>,

    /// When the lease expires.
    pub expires_at: DateTime<Utc>,
}

impl LeaseInfo {
    /// Check if the lease is currently valid (not expired).
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }

    /// Check if this lease is owned by the given orchestrator.
    pub fn is_owned_by(&self, orchestrator_id: &OrchestratorId) -> bool {
        &self.owner == orchestrator_id
    }
}

/// Information about an active orchestrator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratorInfo {
    /// The orchestrator's unique identifier.
    pub id: OrchestratorId,

    /// When the orchestrator last sent a heartbeat.
    pub last_heartbeat: DateTime<Utc>,

    /// Number of runs currently owned by this orchestrator.
    pub active_runs: usize,
}

impl OrchestratorInfo {
    /// Check if the orchestrator is considered alive based on heartbeat timeout.
    pub fn is_alive(&self, heartbeat_timeout: std::time::Duration) -> bool {
        let timeout =
            chrono::Duration::from_std(heartbeat_timeout).unwrap_or(chrono::TimeDelta::MAX);
        Utc::now() - self.last_heartbeat < timeout
    }
}

/// Information needed to recover a run.
///
/// This struct contains all the data needed to restore a run's state
/// from the journal and resume execution.
#[derive(Debug, Clone)]
pub struct RunRecoveryInfo {
    /// The run ID.
    pub run_id: Uuid,

    /// The root run ID (for subflow tracking).
    pub root_run_id: Uuid,

    /// The parent run ID (for subflows).
    pub parent_run_id: Option<Uuid>,

    /// The flow being executed.
    pub flow_id: BlobId,

    /// Input data for each item.
    pub inputs: Vec<ValueRef>,

    /// Variables provided for execution.
    pub variables: HashMap<String, ValueRef>,

    /// Opaque journal offset for replay.
    ///
    /// This is interpreted by the journal backend:
    /// - SQLite: Typically the sequence number to start from
    /// - NATS JetStream: Stream sequence or timestamp
    ///
    /// An empty `Bytes` indicates replay from the beginning.
    pub journal_offset: Bytes,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_id_creation() {
        let id = OrchestratorId::new("orch-1");
        assert_eq!(id.as_str(), "orch-1");
        assert_eq!(id.to_string(), "orch-1");
    }

    #[test]
    fn test_orchestrator_id_from_string() {
        let id: OrchestratorId = "orch-2".into();
        assert_eq!(id.as_str(), "orch-2");

        let id2: OrchestratorId = String::from("orch-3").into();
        assert_eq!(id2.as_str(), "orch-3");
    }

    #[test]
    fn test_orchestrator_id_random() {
        let id1 = OrchestratorId::random();
        let id2 = OrchestratorId::random();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_lease_info_validity() {
        let run_id = Uuid::now_v7();
        let owner = OrchestratorId::new("test");

        // Valid lease (expires in the future)
        let valid_lease = LeaseInfo {
            run_id,
            owner: owner.clone(),
            acquired_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };
        assert!(valid_lease.is_valid());
        assert!(valid_lease.is_owned_by(&owner));
        assert!(!valid_lease.is_owned_by(&OrchestratorId::new("other")));

        // Expired lease
        let expired_lease = LeaseInfo {
            run_id,
            owner,
            acquired_at: Utc::now() - chrono::Duration::hours(2),
            expires_at: Utc::now() - chrono::Duration::hours(1),
        };
        assert!(!expired_lease.is_valid());
    }

    #[test]
    fn test_orchestrator_info_alive() {
        let info = OrchestratorInfo {
            id: OrchestratorId::new("orch-1"),
            last_heartbeat: Utc::now(),
            active_runs: 5,
        };

        assert!(info.is_alive(std::time::Duration::from_secs(30)));

        let stale_info = OrchestratorInfo {
            id: OrchestratorId::new("orch-2"),
            last_heartbeat: Utc::now() - chrono::Duration::minutes(5),
            active_runs: 3,
        };

        assert!(!stale_info.is_alive(std::time::Duration::from_secs(30)));
        assert!(stale_info.is_alive(std::time::Duration::from_secs(600)));
    }
}
