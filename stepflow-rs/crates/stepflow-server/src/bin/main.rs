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

use std::io::Read as _;
use std::path::PathBuf;

use clap::Parser;
use error_stack::{Report, ResultExt as _};
use log::{info, warn};
use stepflow_config::StepflowConfig;
use stepflow_execution::recover_orphaned_runs;
use stepflow_observability::{ObservabilityConfig, init_observability};
use stepflow_server::{orphan_claiming_loop, shutdown_signal};
use stepflow_state::{LeaseManagerExt as _, OrchestratorId};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[derive(Error, Debug)]
pub enum ServerError {
    #[error("Failed to load configuration")]
    ConfigError,
    #[error("Failed to create executor")]
    ExecutorError,
    #[error("Failed to recover pending runs")]
    RecoveryError,
    #[error("Failed to start server")]
    ServerError,
}

type Result<T> = std::result::Result<T, Report<ServerError>>;

/// Stepflow HTTP Server
///
/// Provides a REST API for workflow execution and management.
#[derive(Parser, Debug)]
#[command(name = "stepflow-server")]
#[command(about = "Stepflow HTTP Server", long_about = None)]
struct Args {
    /// Port to bind the HTTP server
    #[arg(short, long, default_value = "7840", env = "STEPFLOW_PORT")]
    port: u16,

    /// Path to stepflow configuration file
    #[arg(short, long, env = "STEPFLOW_CONFIG", conflicts_with = "config_stdin")]
    config: Option<PathBuf>,

    /// Read configuration from stdin as JSON
    #[arg(long, conflicts_with = "config")]
    config_stdin: bool,

    /// Observability configuration
    #[command(flatten)]
    observability: ObservabilityConfig,
}

/// Load configuration from the specified source.
async fn load_config(config_path: Option<PathBuf>, config_stdin: bool) -> Result<StepflowConfig> {
    if config_stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .change_context(ServerError::ConfigError)
            .attach_printable("Failed to read configuration from stdin")?;
        StepflowConfig::load_from_json(&buffer).change_context(ServerError::ConfigError)
    } else if let Some(path) = config_path {
        StepflowConfig::load_from_file(&path)
            .await
            .change_context(ServerError::ConfigError)
    } else {
        Ok(StepflowConfig::default())
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    #[allow(clippy::print_stderr)]
    let guard = {
        let binary_config = stepflow_observability::BinaryObservabilityConfig {
            service_name: "stepflow-server",
            include_run_diagnostic: true,
        };
        match init_observability(&args.observability, binary_config) {
            Ok(guard) => guard,
            Err(e) => {
                eprintln!("Failed to initialize observability: {e:?}");
                std::process::exit(1);
            }
        }
    };

    info!(
        "Starting Stepflow server: port={}, config={:?}",
        args.port, args.config
    );

    let result: Result<()> = async {
        // Load configuration
        let config = load_config(args.config, args.config_stdin).await?;
        let recovery_config = config.recovery.clone();

        // Create executor/environment
        info!("Creating Stepflow executor from configuration");
        let executor = config
            .create_environment()
            .await
            .change_context(ServerError::ExecutorError)
            .attach_printable("Failed to create executor from configuration")?;

        // Generate orchestrator ID (could be configured via env var in the future)
        let orchestrator_id =
            OrchestratorId::new(format!("stepflow-server-{}", uuid::Uuid::now_v7()));

        // Set up cancellation token for graceful shutdown
        let cancel_token = CancellationToken::new();

        // Start background orphan claiming task if lease manager is available
        let orphan_task = if let Some(lease_manager) = executor.lease_manager() {
            // Recover pending runs on startup
            if recovery_config.max_startup_recovery > 0 {
                info!(
                    "Recovering pending runs with orchestrator_id={}",
                    orchestrator_id.as_str()
                );

                let recovery_result = recover_orphaned_runs(
                    &executor,
                    orchestrator_id.clone(),
                    recovery_config.max_startup_recovery,
                )
                .await
                .change_context(ServerError::RecoveryError)?;

                if recovery_result.recovered > 0 || recovery_result.failed > 0 {
                    info!(
                        "Startup recovery complete: {} recovered, {} failed",
                        recovery_result.recovered, recovery_result.failed
                    );
                }
            }

            // Start background orphan claiming loop
            let task = tokio::spawn(orphan_claiming_loop(
                executor.clone(),
                lease_manager.clone(),
                orchestrator_id.clone(),
                recovery_config,
                cancel_token.clone(),
            ));
            Some(task)
        } else {
            None
        };

        // Run server until shutdown signal
        tokio::select! {
            result = stepflow_server::start_server(args.port, executor.clone()) => {
                result
                    .map_err(std::sync::Arc::<dyn std::error::Error + Send + Sync>::from)
                    .change_context(ServerError::ServerError)?;
            }
            _ = shutdown_signal() => {
                info!("Shutdown signal received, stopping server");
            }
        }

        // Graceful shutdown: cancel background task and wait for it
        cancel_token.cancel();
        if let Some(task) = orphan_task {
            info!("Waiting for background tasks to complete...");
            match task.await {
                Ok(()) => {
                    info!("Orphan claiming task exited normally");
                }
                Err(e) => {
                    warn!("Orphan claiming task panicked: {:?}", e);
                }
            }
        }

        // TODO: Release leases for any runs still in progress
        // This requires tracking active runs in the executor, which is not
        // currently implemented. For now, leases will expire naturally.
        // In production with short TTLs (e.g., 30s), this is acceptable.

        info!("Graceful shutdown complete");
        Ok(())
    }
    .await;

    // Close observability guard to flush telemetry before shutdown
    if let Err(e) = guard.close().await {
        log::error!("Failed to flush observability data: {e:?}");
    }

    if let Err(e) = result {
        log::error!("Server error: {e:?}");
        std::process::exit(1);
    }
}
