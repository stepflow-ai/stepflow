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

use clap::Parser;
use error_stack::{Report, ResultExt as _};
use log::info;
use std::path::PathBuf;
use stepflow_config::StepflowConfig;
use stepflow_observability::{ObservabilityConfig, init_observability};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServerError {
    #[error("Failed to load configuration")]
    ConfigError,
    #[error("Failed to create executor")]
    ExecutorError,
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
    #[arg(short, long, env = "STEPFLOW_CONFIG")]
    config: Option<PathBuf>,

    /// Observability configuration
    #[command(flatten)]
    observability: ObservabilityConfig,
}

async fn create_executor(
    config_path: Option<PathBuf>,
) -> Result<std::sync::Arc<stepflow_plugin::StepflowEnvironment>> {
    info!("Creating Stepflow executor from configuration");

    let config = if let Some(path) = config_path {
        StepflowConfig::load_from_file(&path)
            .await
            .change_context(ServerError::ConfigError)?
    } else {
        StepflowConfig::default()
    };

    config
        .create_environment()
        .await
        .change_context(ServerError::ExecutorError)
        .attach_printable("Failed to create executor from configuration")
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

    let result = async {
        let executor = create_executor(args.config).await?;

        stepflow_server::start_server(args.port, executor)
            .await
            .map_err(std::sync::Arc::<dyn std::error::Error + Send + Sync>::from)
            .change_context(ServerError::ServerError)
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
