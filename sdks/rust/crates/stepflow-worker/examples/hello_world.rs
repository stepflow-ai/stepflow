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

//! Hello World — a minimal Stepflow worker example.
//!
//! This example registers a single `/greet` component and runs a worker that
//! pulls tasks from a local Stepflow orchestrator.
//!
//! # Usage
//!
//! 1. Start the Stepflow orchestrator:
//!    ```bash
//!    cd stepflow-rs
//!    cargo run -- serve --port 7840
//!    ```
//!
//! 2. Run this example in another terminal:
//!    ```bash
//!    cd sdks/rust
//!    STEPFLOW_TASKS_URL=http://127.0.0.1:7837 cargo run --example hello_world
//!    ```
//!
//! 3. Submit a run using the CLI or `stepflow-client`:
//!    ```bash
//!    cargo run -- submit --flow examples/greet.yaml --input '{"name": "Alice"}'
//!    ```

use serde::{Deserialize, Serialize};
use stepflow_worker::{ComponentError, ComponentRegistry, Worker, WorkerConfig};

#[derive(Deserialize)]
struct GreetInput {
    name: String,
}

#[derive(Serialize)]
struct GreetOutput {
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up basic logging
    tracing_subscriber::fmt::init();

    // Register components
    let mut registry = ComponentRegistry::new();

    registry.register_fn("/greet", |input: GreetInput, _ctx| async move {
        if input.name.is_empty() {
            return Err(ComponentError::InvalidInput(
                "name must not be empty".to_string(),
            ));
        }
        Ok(GreetOutput {
            message: format!("Hello, {}!", input.name),
        })
    });

    // Configuration is read from environment variables:
    //   STEPFLOW_TASKS_URL, STEPFLOW_QUEUE_NAME, STEPFLOW_MAX_CONCURRENT, …
    let config = WorkerConfig::default();

    println!("Starting worker on {} ...", config.tasks_url);
    println!("Registered components: /greet");
    println!("Press Ctrl-C to stop.");

    Worker::new(registry, config).run().await?;

    Ok(())
}
