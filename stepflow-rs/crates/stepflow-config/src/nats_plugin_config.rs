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

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Configuration for a NATS JetStream transport plugin.
///
/// Each route (or the plugin default) maps to a JetStream **stream** with
/// WorkQueue retention. The stream is the unit of isolation — one per
/// worker pool. Within each stream, tasks are published to a fixed
/// internal subject (`tasks`).
///
/// # Config example
///
/// ```yaml
/// plugins:
///   nats:
///     type: nats
///     url: "nats://localhost:4222"
///     stream: PYTHON_TASKS
///     consumer: python-workers
///     queueTimeoutSecs: 30
///
/// routes:
///   "/python/foo":
///     - plugin: nats
///       stream: FOO_TASKS          # separate worker pool
///   "/python":
///     - plugin: nats               # uses default stream
/// ```
#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NatsPluginConfig {
    /// NATS server URL (e.g., "nats://localhost:4222").
    pub url: String,

    /// Default JetStream stream name. Can be overridden per-route via
    /// the `stream` route param. At least one of plugin-level or
    /// route-level `stream` must be set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,

    /// Durable consumer name for NATS JetStream. Required — workers use
    /// this to create/resume a durable pull consumer within the stream.
    #[serde(default)]
    pub consumer: Option<String>,

    /// Command to launch the worker subprocess.
    /// If not set, the plugin expects an external worker to connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Arguments for the worker subprocess command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables for the worker subprocess.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Maximum time (in seconds) a task can wait in the queue for a
    /// worker to send its first heartbeat. Defaults to 30 seconds.
    #[serde(default = "default_queue_timeout_secs")]
    #[schemars(range(min = 1))]
    pub queue_timeout_secs: u64,

    /// Maximum time (in seconds) from first heartbeat to `CompleteTask`.
    /// Defaults to `null` (no execution timeout).
    #[serde(default)]
    pub execution_timeout_secs: Option<u64>,
}

fn default_queue_timeout_secs() -> u64 {
    30
}
