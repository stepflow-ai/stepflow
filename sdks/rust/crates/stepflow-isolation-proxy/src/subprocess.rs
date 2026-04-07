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

//! Dev mode utilities — spawn a local Python vsock worker subprocess.

use log::info;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::time::sleep;

/// Spawn a Python vsock worker process listening on a Unix socket.
///
/// If `worker_script` is provided, runs `<python> <script> --socket <path> --oneshot`.
/// Otherwise runs `<python> -m stepflow_py.worker.vsock_worker --socket <path> --oneshot`.
///
/// The worker runs in oneshot mode: it processes one task and exits.
pub async fn spawn_python_worker(
    python: &str,
    socket_path: &str,
    worker_script: Option<&str>,
) -> Result<Child, Box<dyn std::error::Error + Send + Sync>> {
    info!("Spawning Python vsock worker on {socket_path}");

    let mut cmd = Command::new(python);
    match worker_script {
        Some(script) => {
            cmd.args([script, "--socket", socket_path, "--oneshot"]);
        }
        None => {
            cmd.args([
                "-m",
                "stepflow_py.worker.vsock_worker",
                "--socket",
                socket_path,
                "--oneshot",
            ]);
        }
    }

    let child = cmd
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn Python worker: {e}"))?;

    Ok(child)
}

/// Wait for a Unix socket file to appear (worker is ready).
pub async fn wait_for_socket(
    path: &str,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let start = std::time::Instant::now();
    loop {
        if tokio::fs::metadata(path).await.is_ok() {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(format!("Timed out waiting for socket at {path}").into());
        }
        sleep(Duration::from_millis(50)).await;
    }
}
