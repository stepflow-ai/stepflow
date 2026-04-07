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

use serde::{Deserialize, Serialize};

/// Configuration for a gRPC transport plugin.
///
/// When instantiated, creates a queue plugin backed by an in-memory
/// task queue. During initialization, registers its queue with the
/// orchestrator's gRPC server and optionally spawns a worker subprocess.
///
/// # Config example
///
/// ```yaml
/// plugins:
///   python:
///     type: grpc
///     command: uv
///     args: ["--project", "../sdks/python/stepflow-py", "run", "stepflow_py", "--grpc"]
///     queueName: python
/// ```
#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrpcPluginConfig {
    /// Command to launch the worker subprocess.
    /// If not set, the plugin expects an external worker to connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Arguments for the worker subprocess command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables for the worker subprocess.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub env: std::collections::HashMap<String, String>,

    /// Queue name the worker uses to receive tasks. Required — must match
    /// `STEPFLOW_QUEUE_NAME` in the worker's environment.
    #[serde(default)]
    pub queue_name: Option<String>,

    /// Maximum time (in seconds) a task can wait in the queue for a
    /// worker to send its first heartbeat. If no worker picks up the task
    /// within this window, it is treated as failed. Must be greater than 0.
    ///
    /// Defaults to 30 seconds.
    #[serde(default = "default_queue_timeout_secs")]
    #[schemars(range(min = 1))]
    pub queue_timeout_secs: u64,

    /// Maximum time (in seconds) from first heartbeat to `CompleteTask`.
    /// If the worker does not complete within this window, the task is
    /// treated as failed. Heartbeat-based crash detection (5s timeout)
    /// provides faster detection of hard worker crashes.
    ///
    /// Defaults to `null` (no execution timeout — relies on heartbeat
    /// crash detection only).
    #[serde(default)]
    pub execution_timeout_secs: Option<u64>,
}

fn default_queue_timeout_secs() -> u64 {
    30
}
