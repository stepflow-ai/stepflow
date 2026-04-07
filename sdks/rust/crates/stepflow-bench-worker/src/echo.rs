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

//! Real worker mode with an `/echo` component via the Rust SDK.

use clap::Args;
use stepflow_worker::{ComponentRegistry, Worker, WorkerConfig};
use tracing::info;

#[derive(Args, Debug)]
pub struct EchoArgs {
    /// URL of the Stepflow tasks gRPC service.
    #[arg(
        long,
        env = "STEPFLOW_TASKS_URL",
        default_value = "http://127.0.0.1:7840"
    )]
    tasks_url: String,

    /// Queue name to pull tasks from.
    #[arg(long, env = "STEPFLOW_QUEUE_NAME", default_value = "rust")]
    queue_name: String,

    /// Max concurrent task executions.
    #[arg(long, env = "STEPFLOW_MAX_CONCURRENT", default_value = "10")]
    max_concurrent: usize,
}

pub async fn run(args: EchoArgs) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        tasks_url = %args.tasks_url,
        queue_name = %args.queue_name,
        max_concurrent = args.max_concurrent,
        "Starting echo worker (Rust SDK)"
    );

    let mut registry = ComponentRegistry::new();

    registry.register_fn("/echo", |input: serde_json::Value, _ctx| async move {
        Ok(input)
    });

    let config = WorkerConfig {
        tasks_url: args.tasks_url.clone(),
        queue_name: args.queue_name,
        max_concurrent: args.max_concurrent,
        // Set orchestrator_url to match tasks_url so the SDK reuses the
        // same gRPC channel for callbacks, avoiding ephemeral port exhaustion.
        orchestrator_url: Some(args.tasks_url),
        ..WorkerConfig::default()
    };

    Worker::new(registry, config)?.run().await?;

    Ok(())
}
