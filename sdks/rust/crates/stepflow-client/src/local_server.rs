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

//! Launch a local Stepflow orchestrator subprocess for testing.
//!
//! Use [`LocalOrchestrator`] when you need a real orchestrator process in tests —
//! it handles startup, port discovery, health checking, and shutdown automatically.
//!
//! # Requirements
//!
//! Set the `STEPFLOW_DEV_BINARY` environment variable to the path of the
//! `stepflow-server` binary before running tests:
//!
//! ```bash
//! STEPFLOW_DEV_BINARY=./stepflow-rs/target/debug/stepflow-server cargo test --features local-server
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "local-server")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use stepflow_client::local_server::{LocalOrchestrator, OrchestratorConfig, PluginConfig, RouteRule};
//! use std::collections::HashMap;
//!
//! let config = OrchestratorConfig {
//!     plugins: {
//!         let mut p = HashMap::new();
//!         p.insert("builtin".to_string(), PluginConfig::Builtin);
//!         p
//!     },
//!     routes: {
//!         let mut r = HashMap::new();
//!         r.insert("/builtin/{*component}".to_string(), vec![RouteRule { plugin: "builtin".to_string() }]);
//!         r
//!     },
//!     ..Default::default()
//! };
//!
//! let orch = LocalOrchestrator::start(config).await?;
//! println!("Orchestrator running at {}", orch.url());
//! // ... use orch.url() with StepflowClient::connect(...)
//! // orch is stopped when dropped
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::ClientError;

// ---------------------------------------------------------------------------
// Config types (serialise to the orchestrator's JSON config format)
// ---------------------------------------------------------------------------

/// Top-level orchestrator configuration passed via `--config-stdin`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorConfig {
    /// Plugin definitions keyed by plugin name.
    pub plugins: HashMap<String, PluginConfig>,
    /// Route rules mapping component path patterns to plugins.
    pub routes: HashMap<String, Vec<RouteRule>>,
    /// Storage configuration (defaults to in-memory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_config: Option<StorageConfig>,
}

/// Plugin configuration variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PluginConfig {
    /// Built-in components (eval, openai, etc.).
    Builtin,
    /// gRPC worker plugin — tasks are delivered via the PullTasks queue.
    Grpc(GrpcPluginConfig),
}

/// gRPC plugin configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcPluginConfig {
    /// Queue name workers must connect to.  Must match `STEPFLOW_QUEUE_NAME`
    /// in the worker's environment / `WorkerConfig::queue_name`.
    pub queue_name: String,
    /// Optional subprocess command to launch the worker.
    /// Leave `None` to expect an external worker to connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Arguments for the subprocess command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Environment variables for the subprocess.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

/// A single route rule that maps a component path pattern to a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    /// Name of the plugin this route maps to.
    pub plugin: String,
}

/// Storage backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StorageConfig {
    /// In-memory storage (default; non-persistent).
    InMemory,
    /// SQLite storage.
    Sqlite {
        #[serde(rename = "databaseUrl")]
        database_url: String,
        #[serde(default)]
        auto_migrate: bool,
    },
}

// ---------------------------------------------------------------------------
// LocalOrchestrator
// ---------------------------------------------------------------------------

/// A locally-running Stepflow orchestrator subprocess.
///
/// Started via [`LocalOrchestrator::start`] and stopped when dropped.
///
/// # Panics
///
/// Panics in `Drop` if the child process cannot be killed (which would indicate
/// an OS-level error).
pub struct LocalOrchestrator {
    child: Child,
    url: String,
}

impl LocalOrchestrator {
    /// Start a local orchestrator subprocess using the `STEPFLOW_DEV_BINARY`
    /// environment variable as the binary path.
    ///
    /// Returns an error if:
    /// - `STEPFLOW_DEV_BINARY` is not set
    /// - The binary cannot be found or launched
    /// - The binary does not announce its port within `startup_timeout`
    /// - The health endpoint does not become reachable within `startup_timeout`
    pub async fn start(config: OrchestratorConfig) -> Result<Self, ClientError> {
        Self::start_with_timeout(config, Duration::from_secs(30)).await
    }

    /// Like [`start`](Self::start) but with a custom startup timeout.
    pub async fn start_with_timeout(
        config: OrchestratorConfig,
        startup_timeout: Duration,
    ) -> Result<Self, ClientError> {
        let binary = std::env::var("STEPFLOW_DEV_BINARY").map_err(|_| {
            ClientError::LocalServer(
                "STEPFLOW_DEV_BINARY environment variable is not set. \
                 Set it to the path of the stepflow binary to run integration tests."
                    .to_string(),
            )
        })?;

        let config_json = serde_json::to_string(&config).map_err(|e| {
            ClientError::LocalServer(format!("Failed to serialize orchestrator config: {e}"))
        })?;

        let mut child = Command::new(&binary)
            .args(["--port", "0", "--config-stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // show orchestrator logs in test output
            .spawn()
            .map_err(|e| {
                ClientError::LocalServer(format!(
                    "Failed to launch orchestrator binary '{binary}': {e}"
                ))
            })?;

        // Write config JSON to stdin then close it
        {
            use std::io::Write;
            let stdin = child.stdin.take().expect("stdin was piped");
            let mut stdin = stdin;
            stdin.write_all(config_json.as_bytes()).map_err(|e| {
                ClientError::LocalServer(format!(
                    "Failed to write config to orchestrator stdin: {e}"
                ))
            })?;
            // stdin dropped here, closing the pipe
        }

        // Read port announcement from stdout (blocking in a blocking thread).
        // Wrap in tokio::time::timeout so we don't block indefinitely if the
        // binary hangs before printing the port line.
        let stdout = child.stdout.take().expect("stdout was piped");
        let port = tokio::time::timeout(
            startup_timeout,
            tokio::task::spawn_blocking(move || read_port_from_stdout(stdout, startup_timeout)),
        )
        .await
        .map_err(|_| {
            ClientError::LocalServer(format!(
                "Timed out after {}s waiting for orchestrator port announcement",
                startup_timeout.as_secs()
            ))
        })?
        .map_err(|e| ClientError::LocalServer(format!("Port reader task panicked: {e}")))?
        .map_err(ClientError::LocalServer)?;

        let url = format!("http://127.0.0.1:{port}");

        // Wait for health endpoint
        wait_for_health(&url, startup_timeout).await.map_err(|e| {
            ClientError::LocalServer(format!("Orchestrator health check failed: {e}"))
        })?;

        Ok(Self { child, url })
    }

    /// The base URL of the orchestrator (e.g. `http://127.0.0.1:54321`).
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The gRPC/tasks service URL (same as [`url`](Self::url) for the default port layout).
    ///
    /// Pass this to `WorkerConfig::tasks_url`.
    pub fn tasks_url(&self) -> &str {
        &self.url
    }
}

impl Drop for LocalOrchestrator {
    fn drop(&mut self) {
        // Best-effort graceful shutdown then kill
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read lines from `stdout` until we see `{"port": N}`, then return the port.
fn read_port_from_stdout(
    stdout: std::process::ChildStdout,
    timeout: Duration,
) -> Result<u16, String> {
    let reader = BufReader::new(stdout);
    let deadline = Instant::now() + timeout;

    for line in reader.lines() {
        if Instant::now() > deadline {
            return Err(format!(
                "Timed out after {}s waiting for port announcement",
                timeout.as_secs()
            ));
        }

        let line = line.map_err(|e| format!("Failed to read orchestrator stdout: {e}"))?;
        let line = line.trim();

        if line.starts_with('{') && line.contains("port") {
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(port) = obj.get("port").and_then(|v| v.as_u64()) {
                    return Ok(port as u16);
                }
            }
        }
    }

    Err("Orchestrator stdout closed without announcing port".to_string())
}

/// Poll the HTTP health endpoint until it responds OK or we time out.
async fn wait_for_health(base_url: &str, timeout: Duration) -> Result<(), String> {
    let health_url = format!("{base_url}/api/v1/health");
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        match tokio::time::timeout(Duration::from_secs(5), reqwest_health_check(&health_url)).await
        {
            Ok(Ok(())) => return Ok(()),
            _ => {}
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "Orchestrator at {base_url} did not become healthy within {}s",
                timeout.as_secs()
            ));
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Make a single HTTP GET request to the health endpoint.
async fn reqwest_health_check(url: &str) -> Result<(), ()> {
    // Use tokio's TcpStream for a minimal HTTP check without pulling in reqwest.
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    // Parse host:port from the URL (e.g. "http://127.0.0.1:12345/api/v1/health")
    let without_scheme = url.trim_start_matches("http://");
    let (host_port, _path) = without_scheme
        .split_once('/')
        .unwrap_or((without_scheme, ""));

    let mut stream = TcpStream::connect(host_port).await.map_err(|_| ())?;

    let request = format!(
        "GET /{} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        url.splitn(4, '/').nth(3).unwrap_or("api/v1/health"),
        host_port
    );

    stream.write_all(request.as_bytes()).await.map_err(|_| ())?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.map_err(|_| ())?;

    // Check for "200 OK" in the status line
    let head = std::str::from_utf8(&response[..response.len().min(64)]).unwrap_or("");
    if head.contains("200") {
        Ok(())
    } else {
        Err(())
    }
}
