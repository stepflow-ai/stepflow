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
use error_stack::{Report, ResultExt};
use std::path::PathBuf;
use stepflow_config::StepflowConfig;
use thiserror::Error;
use tracing::info;

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

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", env = "RUST_LOG")]
    log_level: String,

    /// Log format (json, pretty)
    #[arg(long, default_value = "pretty", env = "STEPFLOW_LOG_FORMAT")]
    log_format: String,
}

fn init_tracing(log_level: &str, log_format: &str) -> Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false);

    match log_format {
        "json" => subscriber.json().init(),
        _ => subscriber.init(),
    }

    Ok(())
}

async fn create_executor(config_path: Option<PathBuf>) -> Result<std::sync::Arc<stepflow_execution::StepflowExecutor>> {
    info!("Creating Stepflow executor from configuration");

    let config = if let Some(path) = config_path {
        StepflowConfig::load_from_file(&path)
            .await
            .change_context(ServerError::ConfigError)?
    } else {
        StepflowConfig::default()
    };

    config
        .create_executor()
        .await
        .change_context(ServerError::ExecutorError)
        .attach_printable("Failed to create executor from configuration")
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Err(e) = init_tracing(&args.log_level, &args.log_format) {
        eprintln!("Failed to initialize tracing: {e:?}");
        std::process::exit(1);
    }

    info!(
        port = args.port,
        config = ?args.config,
        "Starting Stepflow server"
    );

    let result = async {
        let executor = create_executor(args.config).await?;

        stepflow_server::start_server(args.port, executor)
            .await
            .map_err(std::sync::Arc::<dyn std::error::Error + Send + Sync>::from)
            .change_context(ServerError::ServerError)
    }
    .await;

    if let Err(e) = result {
        eprintln!("Server error: {e:?}");
        std::process::exit(1);
    }
}
