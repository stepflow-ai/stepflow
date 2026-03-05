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

use serde::{Deserialize, Deserializer, Serialize};

/// Default: recovery is enabled.
pub const RECOVERY_DEFAULT_ENABLED: bool = true;

/// Default: check for orphans every 30 seconds.
pub const RECOVERY_DEFAULT_CHECK_INTERVAL_SECS: u64 = 30;

/// Default: recover up to 100 runs on startup.
pub const RECOVERY_DEFAULT_MAX_STARTUP_RECOVERY: usize = 100;

/// Default: claim up to 10 orphaned runs per check interval.
pub const RECOVERY_DEFAULT_MAX_CLAIMS_PER_CHECK: usize = 10;

/// Default: orchestrator lease TTL is 30 seconds.
///
/// Heartbeats are sent at `lease_ttl_secs / 3` to keep the lease alive.
/// If an orchestrator stops heartbeating, the lease expires after this duration.
pub const RECOVERY_DEFAULT_LEASE_TTL_SECS: u64 = 30;

/// Default: checkpoint every 1000 journal entries.
pub const RECOVERY_DEFAULT_CHECKPOINT_INTERVAL: usize = 1000;

/// Configuration for run recovery and orphan claiming.
///
/// Controls how the orchestrator handles interrupted runs on startup
/// and during execution.
#[derive(Serialize, Deserialize, Debug, Clone, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct RecoveryConfig {
    /// Whether to enable periodic orphan claiming during execution.
    ///
    /// When enabled, the orchestrator will periodically check for orphaned
    /// runs (from crashed orchestrators) and claim them for execution.
    /// Default: true
    #[serde(deserialize_with = "null_or_default_enabled")]
    pub enabled: bool,

    /// Interval in seconds between orphan check attempts.
    ///
    /// Only used when `enabled` is true. Lower values mean faster recovery
    /// but more overhead. Default: 30 seconds.
    #[serde(deserialize_with = "null_or_default_check_interval_secs")]
    pub check_interval_secs: u64,

    /// Maximum number of runs to recover on startup.
    ///
    /// Limits how many interrupted runs are recovered when the orchestrator
    /// starts. Set to 0 to disable startup recovery. Default: 100.
    #[serde(deserialize_with = "null_or_default_max_startup_recovery")]
    pub max_startup_recovery: usize,

    /// Maximum number of orphaned runs to claim per check interval.
    ///
    /// Limits how many runs are claimed in each periodic check to avoid
    /// overwhelming a single orchestrator. Default: 10.
    #[serde(deserialize_with = "null_or_default_max_claims_per_check")]
    pub max_claims_per_check: usize,

    /// TTL in seconds for the orchestrator lease and heartbeats.
    ///
    /// The heartbeat interval is automatically set to `lease_ttl_secs / 3`.
    /// If an orchestrator stops sending heartbeats, its lease expires after this
    /// duration and its runs become eligible for recovery. Default: 30 seconds.
    #[serde(deserialize_with = "null_or_default_lease_ttl_secs")]
    pub lease_ttl_secs: u64,

    /// Number of journal entries between checkpoints.
    ///
    /// The executor periodically serializes execution state so that recovery
    /// only needs to replay events after the checkpoint instead of from the
    /// beginning. Set to 0 to disable. Default: 1000.
    #[serde(deserialize_with = "null_or_default_checkpoint_interval")]
    pub checkpoint_interval: usize,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            enabled: RECOVERY_DEFAULT_ENABLED,
            check_interval_secs: RECOVERY_DEFAULT_CHECK_INTERVAL_SECS,
            max_startup_recovery: RECOVERY_DEFAULT_MAX_STARTUP_RECOVERY,
            max_claims_per_check: RECOVERY_DEFAULT_MAX_CLAIMS_PER_CHECK,
            lease_ttl_secs: RECOVERY_DEFAULT_LEASE_TTL_SECS,
            checkpoint_interval: RECOVERY_DEFAULT_CHECKPOINT_INTERVAL,
        }
    }
}

fn null_or_default_enabled<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or(RECOVERY_DEFAULT_ENABLED))
}

fn null_or_default_check_interval_secs<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or(RECOVERY_DEFAULT_CHECK_INTERVAL_SECS))
}

fn null_or_default_max_startup_recovery<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<usize, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or(RECOVERY_DEFAULT_MAX_STARTUP_RECOVERY))
}

fn null_or_default_max_claims_per_check<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<usize, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or(RECOVERY_DEFAULT_MAX_CLAIMS_PER_CHECK))
}

fn null_or_default_lease_ttl_secs<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or(RECOVERY_DEFAULT_LEASE_TTL_SECS))
}

fn null_or_default_checkpoint_interval<'de, D: Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or(RECOVERY_DEFAULT_CHECKPOINT_INTERVAL))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_config_null_fields_use_custom_defaults() {
        let json = serde_json::json!({
            "enabled": null,
            "checkIntervalSecs": null,
            "maxStartupRecovery": null,
            "maxClaimsPerCheck": null,
            "leaseTtlSecs": null,
            "checkpointInterval": null,
        });
        let config: RecoveryConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.enabled, RECOVERY_DEFAULT_ENABLED);
        assert_eq!(
            config.check_interval_secs,
            RECOVERY_DEFAULT_CHECK_INTERVAL_SECS
        );
        assert_eq!(
            config.max_startup_recovery,
            RECOVERY_DEFAULT_MAX_STARTUP_RECOVERY
        );
        assert_eq!(
            config.max_claims_per_check,
            RECOVERY_DEFAULT_MAX_CLAIMS_PER_CHECK
        );
        assert_eq!(config.lease_ttl_secs, RECOVERY_DEFAULT_LEASE_TTL_SECS);
        assert_eq!(
            config.checkpoint_interval,
            RECOVERY_DEFAULT_CHECKPOINT_INTERVAL
        );
    }

    #[test]
    fn test_recovery_config_omitted_fields_use_custom_defaults() {
        let json = serde_json::json!({});
        let config: RecoveryConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.enabled, RECOVERY_DEFAULT_ENABLED);
        assert_eq!(
            config.check_interval_secs,
            RECOVERY_DEFAULT_CHECK_INTERVAL_SECS
        );
        assert_eq!(
            config.max_startup_recovery,
            RECOVERY_DEFAULT_MAX_STARTUP_RECOVERY
        );
        assert_eq!(
            config.max_claims_per_check,
            RECOVERY_DEFAULT_MAX_CLAIMS_PER_CHECK
        );
        assert_eq!(config.lease_ttl_secs, RECOVERY_DEFAULT_LEASE_TTL_SECS);
        assert_eq!(
            config.checkpoint_interval,
            RECOVERY_DEFAULT_CHECKPOINT_INTERVAL
        );
    }
}
