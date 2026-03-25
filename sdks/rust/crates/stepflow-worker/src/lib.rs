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

//! # stepflow-worker
//!
//! Rust SDK for implementing [Stepflow](https://stepflow.org) workers — processes
//! that host and execute components on behalf of the Stepflow orchestrator.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use stepflow_worker::{ComponentRegistry, Worker, WorkerConfig, ComponentError};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Deserialize)]
//! struct GreetInput { name: String }
//!
//! #[derive(Serialize)]
//! struct GreetOutput { message: String }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut registry = ComponentRegistry::new();
//!     registry.register_fn("/greet", |input: GreetInput, _ctx| async move {
//!         Ok(GreetOutput {
//!             message: format!("Hello, {}!", input.name),
//!         })
//!     });
//!
//!     // Configuration is read from STEPFLOW_TASKS_URL etc. or CLI args.
//!     Worker::new(registry, WorkerConfig::default())
//!         .run()
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! # Component context
//!
//! Components receive a [`ComponentContext`] that provides access to:
//! - Blob storage ([`ComponentContext::put_blob`] / [`ComponentContext::get_blob`])
//! - Nested flow execution ([`ComponentContext::submit_run`] / [`ComponentContext::evaluate_flow`])
//! - Run metadata (run ID, step ID, attempt number)
//!
//! # Environment variables
//!
//! | Variable | Default | Description |
//! |---|---|---|
//! | `STEPFLOW_TASKS_URL` | `http://127.0.0.1:7837` | Tasks gRPC service URL |
//! | `STEPFLOW_QUEUE_NAME` | `default` | Worker queue name |
//! | `STEPFLOW_MAX_CONCURRENT` | `10` | Max concurrent tasks |
//! | `STEPFLOW_MAX_RETRIES` | `10` | Max reconnect attempts |
//! | `STEPFLOW_SHUTDOWN_GRACE_SECS` | `30` | Graceful shutdown timeout |
//! | `STEPFLOW_BLOB_URL` | *(none)* | Blob API URL override |
//! | `STEPFLOW_ORCHESTRATOR_URL` | *(none)* | OrchestratorService URL override |

pub mod context;
pub mod error;
pub mod registry;
pub mod task_handler;
pub mod worker;

pub use context::{ComponentContext, RunStatus};
pub use error::{ComponentError, ContextError, WorkerError};
pub use registry::{Component, ComponentRegistry};
pub use worker::{Worker, WorkerConfig};
