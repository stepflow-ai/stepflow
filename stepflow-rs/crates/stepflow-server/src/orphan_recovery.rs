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

//! Background orphan recovery for durable workflow execution.
//!
//! This module provides functionality to periodically check for and recover
//! orphaned runs (runs whose orchestrator crashed or lost its lease).

use std::sync::Arc;
use std::time::Duration;

use log::{info, warn};
use stepflow_config::RecoveryConfig;
use stepflow_execution::recover_orphaned_runs;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{LeaseManagerExt as _, OrchestratorId};
use tokio_util::sync::CancellationToken;

/// Background task that periodically checks for and claims orphaned runs.
///
/// This task can operate in two modes:
/// - Push-based: Uses `watch_orphans()` for real-time notifications (preferred)
/// - Polling-based: Falls back to periodic polling if push is unavailable
///
/// The task respects the cancellation token for graceful shutdown.
pub async fn orphan_claiming_loop(
    env: Arc<StepflowEnvironment>,
    orchestrator_id: OrchestratorId,
    config: RecoveryConfig,
    cancel_token: CancellationToken,
) {
    if !config.enabled {
        info!("Periodic orphan claiming is disabled");
        return;
    }

    let lease_manager = env.lease_manager();

    let interval = Duration::from_secs(config.check_interval_secs);
    info!(
        "Starting orphan claiming loop: interval={}s, max_claims={}",
        config.check_interval_secs, config.max_claims_per_check
    );

    // Check if the lease manager supports push-based orphan notification
    if let Some(mut orphan_receiver) = lease_manager.watch_orphans() {
        info!("Using push-based orphan notification");
        run_push_mode(&env, &orchestrator_id, &cancel_token, &mut orphan_receiver).await;
    } else {
        // Fall back to polling mode
        info!("Using polling-based orphan detection");
        run_polling_mode(&env, &orchestrator_id, &config, interval, &cancel_token).await;
    }

    info!("Orphan claiming loop exiting");
}

/// Run the orphan claiming loop in push-based mode.
///
/// Push notifications are treated as wake-up signals indicating orphans may be available.
/// We don't target the specific notified run because it may have already been claimed
/// by another orchestrator by the time we process the notification.
async fn run_push_mode(
    env: &Arc<StepflowEnvironment>,
    orchestrator_id: &OrchestratorId,
    cancel_token: &CancellationToken,
    orphan_receiver: &mut tokio::sync::mpsc::UnboundedReceiver<uuid::Uuid>,
) {
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Orphan claiming loop cancelled (watch mode)");
                break;
            }
            Some(_notified_run_id) = orphan_receiver.recv() => {
                // Notification is a wake-up signal; claim whatever orphans are available
                handle_orphan_recovery(env, orchestrator_id).await;
            }
        }
    }
}

/// Run the orphan claiming loop in polling mode.
async fn run_polling_mode(
    env: &Arc<StepflowEnvironment>,
    orchestrator_id: &OrchestratorId,
    config: &RecoveryConfig,
    interval: Duration,
    cancel_token: &CancellationToken,
) {
    let mut interval_timer = tokio::time::interval(interval);
    interval_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Orphan claiming loop cancelled (polling mode)");
                break;
            }
            _ = interval_timer.tick() => {
                handle_periodic_recovery(env, orchestrator_id, config.max_claims_per_check).await;
            }
        }
    }
}

/// Handle recovery triggered by orphan notification (push mode).
///
/// Attempts to claim and recover one orphaned run. The notification serves as a
/// wake-up signal; we recover whatever orphan is available, not necessarily the
/// one that triggered the notification.
async fn handle_orphan_recovery(env: &Arc<StepflowEnvironment>, orchestrator_id: &OrchestratorId) {
    match recover_orphaned_runs(env, orchestrator_id.clone(), 1).await {
        Ok(result) => {
            if result.recovered > 0 {
                info!(
                    "Recovered {} orphaned run(s): {:?}",
                    result.recovered, result.recovered_run_ids
                );
            }
        }
        Err(e) => {
            warn!("Orphan recovery failed: {:?}", e);
        }
    }
}

/// Handle periodic recovery check.
async fn handle_periodic_recovery(
    env: &Arc<StepflowEnvironment>,
    orchestrator_id: &OrchestratorId,
    max_claims: usize,
) {
    match recover_orphaned_runs(env, orchestrator_id.clone(), max_claims).await {
        Ok(result) => {
            if result.recovered > 0 || result.failed > 0 {
                info!(
                    "Periodic recovery: {} recovered, {} failed",
                    result.recovered, result.failed
                );
            }
        }
        Err(e) => {
            warn!("Periodic orphan claiming failed: {:?}", e);
        }
    }
}
