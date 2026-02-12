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

use std::sync::Arc;
use std::time::Duration;

use error_stack::{Result, ResultExt as _};
use serde::{Deserialize, Serialize};
use stepflow_state::{LeaseManager, NoOpLeaseManager};

use crate::ConfigError;

/// Configuration for the lease manager used in distributed deployments.
///
/// The lease manager handles run ownership in multi-orchestrator scenarios,
/// ensuring only one orchestrator executes a given run at a time.
#[derive(Serialize, Deserialize, Debug, Default, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LeaseManagerConfig {
    /// No-op lease management (single orchestrator mode).
    ///
    /// This is the default for single-node deployments where there's no need
    /// for distributed coordination. All lease requests succeed immediately.
    #[default]
    #[serde(alias = "none")]
    NoOp,

    /// etcd-backed lease management for distributed deployments.
    ///
    /// Uses etcd v3 for distributed coordination with one etcd lease per
    /// orchestrator. Provides automatic cleanup on crash, push-based orphan
    /// detection, and efficient `release_all` via lease revocation.
    #[cfg(feature = "etcd")]
    #[serde(alias = "etcd")]
    Etcd(stepflow_state_etcd::EtcdLeaseManagerConfig),

    /// etcd-backed lease management (requires the `etcd` feature flag).
    #[cfg(not(feature = "etcd"))]
    #[serde(alias = "etcd")]
    Etcd(serde_json::Value),
}

impl LeaseManagerConfig {
    /// Create a LeaseManager instance from this configuration.
    ///
    /// The `lease_ttl` is passed to the underlying implementation and controls
    /// how long a lease survives without heartbeats.
    pub async fn create_lease_manager(
        &self,
        lease_ttl: Duration,
    ) -> Result<Arc<dyn LeaseManager>, ConfigError> {
        match self {
            LeaseManagerConfig::NoOp => Ok(Arc::new(NoOpLeaseManager::new(lease_ttl))),
            #[cfg(feature = "etcd")]
            LeaseManagerConfig::Etcd(config) => {
                let manager = stepflow_state_etcd::EtcdLeaseManager::connect(config, lease_ttl)
                    .await
                    .change_context(ConfigError::Configuration)
                    .attach_printable("Failed to connect to etcd for lease management")?;
                Ok(Arc::new(manager))
            }
            #[cfg(not(feature = "etcd"))]
            LeaseManagerConfig::Etcd(_) => Err(error_stack::report!(ConfigError::Configuration))
                .attach_printable(
                    "etcd lease manager requires the `etcd` feature flag \
                     (compile stepflow with `--features etcd`)",
                ),
        }
    }
}
