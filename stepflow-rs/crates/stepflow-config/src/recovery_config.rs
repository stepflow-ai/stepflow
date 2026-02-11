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
use stepflow_state::DEFAULT_LEASE_TTL_SECS;

/// Default: recovery is enabled.
pub const RECOVERY_DEFAULT_ENABLED: bool = true;

/// Default: check for orphans every 30 seconds.
pub const RECOVERY_DEFAULT_CHECK_INTERVAL_SECS: u64 = 30;

/// Default: recover up to 100 runs on startup.
pub const RECOVERY_DEFAULT_MAX_STARTUP_RECOVERY: usize = 100;

/// Default: claim up to 10 orphaned runs per check interval.
pub const RECOVERY_DEFAULT_MAX_CLAIMS_PER_CHECK: usize = 10;

/// Configuration for run recovery and orphan claiming.
///
/// Controls how the orchestrator handles interrupted runs on startup
/// and during execution.
#[derive(Serialize, Deserialize, Debug, Clone, utoipa::ToSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct RecoveryConfig {
    /// Whether to enable periodic orphan claiming during execution.
    ///
    /// When enabled, the orchestrator will periodically check for orphaned
    /// runs (from crashed orchestrators) and claim them for execution.
    /// Default: true
    pub enabled: bool,

    /// Interval in seconds between orphan check attempts.
    ///
    /// Only used when `enabled` is true. Lower values mean faster recovery
    /// but more overhead. Default: 30 seconds.
    pub check_interval_secs: u64,

    /// Maximum number of runs to recover on startup.
    ///
    /// Limits how many interrupted runs are recovered when the orchestrator
    /// starts. Set to 0 to disable startup recovery. Default: 100.
    pub max_startup_recovery: usize,

    /// Maximum number of orphaned runs to claim per check interval.
    ///
    /// Limits how many runs are claimed in each periodic check to avoid
    /// overwhelming a single orchestrator. Default: 10.
    pub max_claims_per_check: usize,

    /// TTL in seconds for the orchestrator lease and heartbeats.
    ///
    /// The heartbeat interval is automatically set to `lease_ttl_secs / 3`.
    /// If an orchestrator stops sending heartbeats, its lease expires after this
    /// duration and its runs become eligible for recovery. Default: 30 seconds.
    pub lease_ttl_secs: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            enabled: RECOVERY_DEFAULT_ENABLED,
            check_interval_secs: RECOVERY_DEFAULT_CHECK_INTERVAL_SECS,
            max_startup_recovery: RECOVERY_DEFAULT_MAX_STARTUP_RECOVERY,
            max_claims_per_check: RECOVERY_DEFAULT_MAX_CLAIMS_PER_CHECK,
            lease_ttl_secs: DEFAULT_LEASE_TTL_SECS,
        }
    }
}
