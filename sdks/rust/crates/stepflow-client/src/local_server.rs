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

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::ClientError;

/// Default number of log lines retained in the ring buffer.
const DEFAULT_LOG_BUFFER_CAPACITY: usize = 500;

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
/// Orchestrator stdout is captured into a ring buffer after port discovery
/// so that recent log lines can be inspected on failure via [`recent_logs`](Self::recent_logs).
///
/// # Panics
///
/// Panics in `Drop` if the child process cannot be killed (which would indicate
/// an OS-level error).
pub struct LocalOrchestrator {
    child: Child,
    url: String,
    log_buffer: Arc<Mutex<VecDeque<String>>>,
    dump_logs_on_panic: bool,
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

        let child = Command::new(&binary)
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

        // Guard that kills the child process if we bail out before
        // successfully constructing LocalOrchestrator (which has its own Drop).
        let mut guard = ChildKillGuard::new(child);

        // Write config JSON to stdin then close it
        {
            use std::io::Write;
            let child = guard.0.as_mut().expect("guard is armed");
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
        let stdout = guard
            .0
            .as_mut()
            .expect("guard is armed")
            .stdout
            .take()
            .expect("stdout was piped");
        let (port, reader) = tokio::time::timeout(
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

        // Drain remaining stdout into a ring buffer so the pipe stays open
        // (avoiding broken-pipe errors) and logs are available for debugging.
        let log_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(
            DEFAULT_LOG_BUFFER_CAPACITY,
        )));
        let buf = Arc::clone(&log_buffer);
        std::thread::spawn(move || drain_stdout(reader, buf));

        let url = format!("http://127.0.0.1:{port}");

        // Wait for health endpoint
        wait_for_health(&url, startup_timeout).await.map_err(|e| {
            ClientError::LocalServer(format!("Orchestrator health check failed: {e}"))
        })?;

        // Disarm the guard — LocalOrchestrator::drop takes over from here.
        let child = guard.disarm();

        Ok(Self {
            child,
            url,
            log_buffer,
            dump_logs_on_panic: true,
        })
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

    /// Set whether to dump recent orchestrator logs to stderr when the test
    /// panics (default: `true`).
    pub fn set_dump_logs_on_panic(&mut self, enabled: bool) {
        self.dump_logs_on_panic = enabled;
    }

    /// Return the most recent `n` log lines captured from the orchestrator's stdout.
    ///
    /// The orchestrator's stdout is continuously drained into a fixed-size ring
    /// buffer after port discovery. This method returns the last `n` lines
    /// (or fewer if not enough lines have been captured yet).
    pub fn recent_logs(&self, n: usize) -> Vec<String> {
        let buf = self.log_buffer.lock().expect("log buffer lock poisoned");
        let len = buf.len();
        buf.iter().skip(len.saturating_sub(n)).cloned().collect()
    }
}

impl Drop for LocalOrchestrator {
    fn drop(&mut self) {
        // Best-effort graceful shutdown then kill
        let _ = self.child.kill();
        let _ = self.child.wait();

        // When unwinding from a panic (e.g. a failed assertion in a test),
        // dump recent orchestrator logs to stderr so the failure context is
        // visible in `cargo test` output.
        if self.dump_logs_on_panic && std::thread::panicking() {
            let buf = self.log_buffer.lock().expect("log buffer lock poisoned");
            if !buf.is_empty() {
                eprintln!("\n--- Recent orchestrator logs ({} lines) ---", buf.len());
                for line in buf.iter() {
                    eprintln!("  {line}");
                }
                eprintln!("--- End orchestrator logs ---\n");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Guard that kills a child process on drop unless [`disarm`](Self::disarm) is called.
///
/// Used during `start_with_timeout` to ensure the subprocess is cleaned up if
/// any step after `spawn` fails (e.g. health check timeout).
struct ChildKillGuard(Option<Child>);

impl ChildKillGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    /// Disarm the guard and return the inner `Child`.
    fn disarm(&mut self) -> Child {
        self.0.take().expect("guard already disarmed")
    }
}

impl Drop for ChildKillGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Read lines from `stdout` until we see `{"port": N}`, then return the port
/// together with the remaining `BufReader` so the caller can continue draining.
fn read_port_from_stdout(
    stdout: std::process::ChildStdout,
    timeout: Duration,
) -> Result<(u16, BufReader<std::process::ChildStdout>), String> {
    let mut reader = BufReader::new(stdout);
    let deadline = Instant::now() + timeout;
    let mut line_buf = String::new();

    loop {
        if Instant::now() > deadline {
            return Err(format!(
                "Timed out after {}s waiting for port announcement",
                timeout.as_secs()
            ));
        }

        line_buf.clear();
        let n = reader
            .read_line(&mut line_buf)
            .map_err(|e| format!("Failed to read orchestrator stdout: {e}"))?;
        if n == 0 {
            return Err("Orchestrator stdout closed without announcing port".to_string());
        }

        let line = line_buf.trim();

        if line.starts_with('{')
            && line.contains("port")
            && let Ok(obj) = serde_json::from_str::<serde_json::Value>(line)
            && let Some(port) = obj.get("port").and_then(|v| v.as_u64())
        {
            return Ok((port as u16, reader));
        }
    }
}

/// Continuously read lines from the orchestrator's stdout into a ring buffer.
///
/// Runs on a background thread until the pipe is closed (i.e. the process exits).
fn drain_stdout(
    reader: BufReader<std::process::ChildStdout>,
    buffer: Arc<Mutex<VecDeque<String>>>,
) {
    for line in reader.lines() {
        match line {
            Ok(line) => {
                let mut buf = buffer.lock().expect("log buffer lock poisoned");
                if buf.len() == DEFAULT_LOG_BUFFER_CAPACITY {
                    buf.pop_front();
                }
                buf.push_back(line);
            }
            // Pipe closed — child process exited.
            Err(_) => break,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a pre-populated log buffer for unit tests.
    fn make_log_buffer(lines: &[&str]) -> Arc<Mutex<VecDeque<String>>> {
        let buf: VecDeque<String> = lines.iter().map(|s| s.to_string()).collect();
        Arc::new(Mutex::new(buf))
    }

    #[test]
    fn test_recent_logs_returns_last_n_lines() {
        let buffer = make_log_buffer(&["line1", "line2", "line3", "line4", "line5"]);
        let buf = buffer.lock().unwrap();
        let len = buf.len();
        let recent: Vec<String> = buf.iter().skip(len.saturating_sub(3)).cloned().collect();
        assert_eq!(recent, vec!["line3", "line4", "line5"]);
    }

    #[test]
    fn test_recent_logs_fewer_than_n() {
        let buffer = make_log_buffer(&["only_line"]);
        let buf = buffer.lock().unwrap();
        let len = buf.len();
        let recent: Vec<String> = buf.iter().skip(len.saturating_sub(10)).cloned().collect();
        assert_eq!(recent, vec!["only_line"]);
    }

    #[test]
    fn test_recent_logs_empty_buffer() {
        let buffer = make_log_buffer(&[]);
        let buf = buffer.lock().unwrap();
        let len = buf.len();
        let recent: Vec<String> = buf.iter().skip(len.saturating_sub(5)).cloned().collect();
        assert!(recent.is_empty());
    }

    #[test]
    fn test_ring_buffer_caps_at_capacity() {
        let mut buf = VecDeque::with_capacity(DEFAULT_LOG_BUFFER_CAPACITY);
        let total_lines = DEFAULT_LOG_BUFFER_CAPACITY + 100;

        // Simulate the same logic drain_stdout uses.
        for i in 0..total_lines {
            if buf.len() == DEFAULT_LOG_BUFFER_CAPACITY {
                buf.pop_front();
            }
            buf.push_back(format!("line {i}"));
        }

        assert_eq!(buf.len(), DEFAULT_LOG_BUFFER_CAPACITY);
        // The first retained line should be line 100 (after evicting 0..99).
        assert_eq!(buf.front().unwrap(), "line 100");
        assert_eq!(buf.back().unwrap(), &format!("line {}", total_lines - 1));
    }

    #[test]
    fn test_child_kill_guard_kills_on_drop() {
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("failed to spawn sleep");

        let id = child.id();
        {
            let _guard = ChildKillGuard::new(child);
            // guard dropped here — should kill the child
        }

        // After the guard kills + waits, the pid should be reaped.
        // Verify by trying to send signal 0 (existence check).
        std::thread::sleep(Duration::from_millis(50));
        let status = Command::new("kill")
            .args(["-0", &id.to_string()])
            .status();
        assert!(
            status.is_err() || !status.unwrap().success(),
            "process should no longer exist"
        );
    }

    #[test]
    fn test_child_kill_guard_disarm_does_not_kill() {
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("failed to spawn sleep");

        let mut guard = ChildKillGuard::new(child);
        let mut child = guard.disarm();
        // guard dropped here — should be a no-op since it was disarmed

        // Process should still be alive.
        let alive = Command::new("kill")
            .args(["-0", &child.id().to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(alive, "process should still be alive after disarm");

        // Clean up.
        let _ = child.kill();
        let _ = child.wait();
    }
}
