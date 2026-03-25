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

//! # stepflow-client
//!
//! Rust client SDK for the [Stepflow](https://stepflow.org) orchestrator.
//!
//! Provides two main capabilities:
//!
//! - **Flow authoring** — build [`Flow`] definitions programmatically using
//!   [`FlowBuilder`] and [`ValueExpr`]
//! - **Orchestrator client** — store flows, submit runs, and monitor results via
//!   [`StepflowClient`]
//!
//! # Example: store and run a flow
//!
//! ```rust,no_run
//! use stepflow_client::{StepflowClient, FlowBuilder, ValueExpr};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let mut client = StepflowClient::connect("http://localhost:7840").await?;
//!
//! let mut builder = FlowBuilder::new();
//! builder.add_step(
//!     "process",
//!     "/python/my_func",
//!     ValueExpr::object([("data", FlowBuilder::input().into())]),
//! );
//! let flow = builder
//!     .output(FlowBuilder::step("process").field("result"))
//!     .build()?;
//!
//! let flow_id = client.store_flow(&flow).await?;
//! let output = client.run(&flow_id, serde_json::json!({"key": "value"})).await?;
//! println!("{output}");
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod error;
pub mod flow;
#[cfg(feature = "local-server")]
pub mod local_server;

pub use client::{
    ComponentInfo, FlowVariable, ListComponentsResult, RunStatus, StatusEventStream, StepflowClient,
};
pub use error::{BuilderError, ClientError};
pub use flow::{ErrorAction, Flow, FlowBuilder, InputRef, Step, StepHandle, StepRef, ValueExpr};
// Re-export StatusEvent so callers don't need to depend on stepflow-proto directly.
pub use stepflow_proto::StatusEvent;
