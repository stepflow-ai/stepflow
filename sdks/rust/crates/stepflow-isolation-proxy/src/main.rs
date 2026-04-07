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

//! Stepflow Isolation Proxy — dispatches tasks to sandboxed workers.
//!
//! Pulls tasks from the orchestrator's PullTasks gRPC stream and forwards
//! each task to an isolated worker via vsock/Unix socket. The worker handles
//! the full task lifecycle (claim, heartbeat, execute, complete) by talking
//! directly to the orchestrator.
//!
//! Supports multiple isolation backends:
//! - `subprocess`: Spawn a Python worker per task, communicate via Unix socket.
//! - `firecracker`: Dispatch to Firecracker microVMs via vsock (Phase 3).

mod config;
mod dispatch;
mod subprocess;
mod vm;
mod vsock;

use clap::Parser;
use config::ProxyConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = ProxyConfig::parse();
    dispatch::run(config).await
}
