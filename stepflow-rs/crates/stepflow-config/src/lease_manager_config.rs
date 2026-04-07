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

use serde::{Deserialize, Serialize};

use crate::EtcdLeaseManagerConfig;

/// Configuration for the lease manager used in distributed deployments.
///
/// The lease manager handles run ownership in multi-orchestrator scenarios,
/// ensuring only one orchestrator executes a given run at a time.
#[derive(Serialize, Deserialize, Debug, Default, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
#[schemars(transform = stepflow_flow::discriminator_schema::AddDiscriminator::new("type"))]
pub enum LeaseManagerConfig {
    /// No-op lease management (single orchestrator mode).
    #[default]
    #[serde(alias = "none")]
    #[schemars(title = "NoOpLeaseManager")]
    NoOp,

    /// etcd-backed lease management for distributed deployments.
    #[serde(alias = "etcd")]
    #[schemars(title = "EtcdLeaseManager")]
    Etcd(EtcdLeaseManagerConfig),
}
