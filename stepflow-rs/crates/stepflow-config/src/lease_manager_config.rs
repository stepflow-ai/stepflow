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

use serde::{Deserialize, Serialize};
use stepflow_state::{LeaseManager, NoOpLeaseManager};

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
    // Future variants:
    // Etcd(EtcdLeaseManagerConfig),
}

impl LeaseManagerConfig {
    /// Create a LeaseManager instance from this configuration.
    pub fn create_lease_manager(&self) -> Arc<dyn LeaseManager> {
        match self {
            LeaseManagerConfig::NoOp => Arc::new(NoOpLeaseManager::new()),
        }
    }
}
