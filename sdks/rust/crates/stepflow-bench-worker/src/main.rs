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

//! Benchmark worker for Stepflow throughput testing.
//!
//! Two modes:
//!
//! **`instant`** — Pulls tasks via raw gRPC and completes them immediately,
//! bypassing the worker SDK. Measures orchestrator dispatch ceiling.
//!
//! **`echo`** — Runs a real worker (via the Rust SDK) with an `/echo` component
//! that returns input unchanged. Measures Rust worker overhead.
//!
//! # Usage
//!
//! ```bash
//! # Instant completion (orchestrator ceiling)
//! stepflow-bench-worker instant --queue-name bench --workers 10
//!
//! # Real Rust worker with echo component
//! stepflow-bench-worker echo --queue-name rust
//! ```

use clap::{Parser, Subcommand};

mod echo;
mod instant;

#[derive(Parser, Debug)]
#[command(name = "stepflow-bench-worker")]
#[command(about = "Benchmark worker for Stepflow throughput testing")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Pull tasks and complete them instantly (bypass worker SDK).
    /// Measures orchestrator dispatch ceiling.
    Instant(instant::InstantArgs),

    /// Run a real worker with an /echo component (via Rust SDK).
    /// Measures Rust worker overhead.
    Echo(echo::EchoArgs),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Instant(args) => instant::run(args).await,
        Command::Echo(args) => echo::run(args).await,
    }
}
