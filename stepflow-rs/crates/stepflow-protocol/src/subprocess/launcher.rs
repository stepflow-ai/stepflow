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

use std::{ffi::OsString, path::PathBuf, process::Stdio, time::Duration};

use crate::error::{Result, TransportError};
use crate::plugin::HealthCheckConfig;
use error_stack::ResultExt as _;
use indexmap::IndexMap;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::process::Child;

#[cfg(unix)]
use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};

/// Port announcement JSON format from component server.
#[derive(Deserialize)]
struct PortAnnouncement {
    port: u16,
}

/// Configuration for launching a subprocess HTTP server.
#[derive(Clone)]
pub struct SubprocessLauncher {
    working_directory: PathBuf,
    command: PathBuf,
    args: Vec<OsString>,
    env: IndexMap<String, String>,
    health_check: HealthCheckConfig,
}

/// Handle to a running subprocess.
pub struct SubprocessHandle {
    #[allow(dead_code)] // Kept for kill_on_drop behavior
    child: Child,
    url: String,
    output_task: tokio::task::JoinHandle<()>,
    #[cfg(unix)]
    pgid: i32,
}

impl SubprocessHandle {
    /// Get the URL of the running server.
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for SubprocessHandle {
    fn drop(&mut self) {
        log::debug!("SubprocessHandle being dropped, cleaning up");
        // Abort monitoring task when the handle is dropped
        self.output_task.abort();

        // Kill the entire process group to ensure grandchildren are terminated.
        // kill_on_drop only kills the direct child, not grandchildren (e.g., python spawned by uv).
        #[cfg(unix)]
        if self.pgid > 0 {
            log::info!("Killing subprocess process group {}", self.pgid);
            if let Err(e) = signal::killpg(Pid::from_raw(self.pgid), Signal::SIGTERM) {
                if e != nix::errno::Errno::ESRCH {
                    log::warn!("Failed to kill process group {}: {}", self.pgid, e);
                }
            } else {
                log::info!("Successfully sent SIGTERM to process group {}", self.pgid);
            }
        }
    }
}

impl SubprocessLauncher {
    /// Create a new subprocess launcher.
    ///
    /// Validates that the command exists and resolves its full path.
    pub fn try_new(
        working_directory: PathBuf,
        command: String,
        args: Vec<String>,
        env: IndexMap<String, String>,
        health_check: Option<HealthCheckConfig>,
    ) -> Result<Self> {
        let command = which::WhichConfig::new()
            .system_path_list()
            .custom_cwd(working_directory.clone())
            .binary_name(command.clone().into())
            .first_result()
            .change_context_lazy(|| TransportError::MissingCommand(command))?;
        error_stack::ensure!(command.is_file(), TransportError::InvalidCommand(command));

        Ok(Self {
            working_directory,
            command,
            args: args.into_iter().map(|s| s.into()).collect(),
            env,
            health_check: health_check.unwrap_or_default(),
        })
    }

    /// Launch the subprocess and wait for it to become ready.
    ///
    /// This method:
    /// 1. Spawns the subprocess
    /// 2. Reads the port announcement from stdout (JSON: {"port": N})
    /// 3. Polls the /health endpoint until the server responds
    /// 4. Returns a handle with the server URL
    pub async fn launch(self) -> Result<SubprocessHandle> {
        let mut child = self.spawn()?;

        #[cfg(unix)]
        let pgid = child.id().map(|pid| pid as i32).unwrap_or(0);

        // Create a oneshot channel for port announcement
        let (port_tx, port_rx) = tokio::sync::oneshot::channel();

        // Start output monitoring task which will read and send the port
        let output_task = self.spawn_output_monitor(&mut child, port_tx);

        // Wait for port announcement from the monitor task
        let port = tokio::time::timeout(Duration::from_secs(30), port_rx)
            .await
            .change_context(TransportError::Spawn)
            .attach_printable("Timeout waiting for port announcement from subprocess")?
            .change_context(TransportError::Spawn)
            .attach_printable("Output monitor task closed without sending port")?;

        let url = format!("http://127.0.0.1:{}", port);

        // Wait for health check
        self.wait_for_health(&url).await?;

        log::info!("Subprocess HTTP server ready at {}", url);

        Ok(SubprocessHandle {
            child,
            url,
            output_task,
            #[cfg(unix)]
            pgid,
        })
    }

    /// Spawn the subprocess.
    fn spawn(&self) -> Result<Child> {
        let env: std::collections::HashMap<String, String> = std::env::vars().collect();

        // Substitute environment variables in command arguments
        let mut substituted_args = Vec::new();
        for arg in &self.args {
            let substituted_arg = subst::substitute(&arg.to_string_lossy(), &env)
                .change_context_lazy(|| {
                    TransportError::InvalidEnvironmentVariable(arg.to_string_lossy().to_string())
                })?;
            substituted_args.push(substituted_arg);
        }

        let mut command = tokio::process::Command::new(&self.command);
        command
            .args(&substituted_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Create a new process group on Unix so we can kill the entire tree in Drop.
        // This ensures grandchild processes (e.g., python spawned by uv) are also terminated.
        #[cfg(unix)]
        // SAFETY: This sets up a new process group before exec, which is safe.
        unsafe {
            command.pre_exec(|| {
                nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
                    .map_err(std::io::Error::other)?;
                Ok(())
            });
        }

        command.current_dir(
            std::env::current_dir()
                .unwrap()
                .join(&self.working_directory),
        );

        // Only pass explicit environment variables through.
        command.env_clear();

        for (key, template) in self.env.iter() {
            // Substitute environment variables in the template
            let substituted_value =
                subst::substitute(template, &env).change_context_lazy(|| {
                    TransportError::InvalidEnvironmentVariable(template.clone())
                })?;
            command.env(key, substituted_value);
        }

        log::info!("Spawning subprocess HTTP server: {:?}", command);

        match command.spawn() {
            Ok(child) => Ok(child),
            Err(e) => {
                log::error!(
                    "Failed to spawn subprocess '{} {:?}': {e}",
                    self.command.display(),
                    self.args
                );

                Err(
                    error_stack::report!(TransportError::Spawn).attach_printable(format!(
                        "Failed to spawn '{} {:?}",
                        self.command.display(),
                        self.args
                    )),
                )
            }
        }
    }

    /// Spawn a task to monitor stdout and stderr, extracting port from first line of stdout.
    fn spawn_output_monitor(
        &self,
        child: &mut Child,
        port_tx: tokio::sync::oneshot::Sender<u16>,
    ) -> tokio::task::JoinHandle<()> {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let verbose = std::env::var("STEPFLOW_COMPONENT_STDERR")
            .map(|v| v.to_lowercase() == "verbose")
            .unwrap_or(false);

        tokio::spawn(async move {
            let mut stdout_reader = stdout.map(|s| BufReader::new(s).lines());
            let mut stderr_reader = stderr.map(|s| BufReader::new(s).lines());
            let mut port_tx = Some(port_tx);

            loop {
                tokio::select! {
                    // Read from stdout
                    result = async {
                        if let Some(ref mut reader) = stdout_reader {
                            reader.next_line().await
                        } else {
                            std::future::pending().await
                        }
                    } => {
                        match result {
                            Ok(Some(line)) => {
                                // First line should be port announcement
                                if let Some(tx) = port_tx.take() {
                                    if let Ok(announcement) = serde_json::from_str::<PortAnnouncement>(&line) {
                                        log::info!("Subprocess announced port: {}", announcement.port);
                                        let _ = tx.send(announcement.port);
                                        continue; // Don't log the port announcement line
                                    } else {
                                        log::warn!("Expected port announcement, got: {}", line);
                                        // Put sender back in case next line is the announcement
                                        port_tx = Some(tx);
                                    }
                                }
                                // Log subsequent stdout lines
                                if verbose {
                                    log::info!("Component stdout: {}", line);
                                } else {
                                    log::debug!("Component stdout: {}", line);
                                }
                            }
                            Ok(None) => break, // EOF
                            Err(_) => break,   // Error
                        }
                    }
                    // Read from stderr
                    result = async {
                        if let Some(ref mut reader) = stderr_reader {
                            reader.next_line().await
                        } else {
                            std::future::pending().await
                        }
                    } => {
                        match result {
                            Ok(Some(line)) => {
                                if verbose {
                                    log::info!("Component stderr: {}", line);
                                } else {
                                    log::debug!("Component stderr: {}", line);
                                }
                            }
                            Ok(None) => break, // EOF
                            Err(_) => break,   // Error
                        }
                    }
                }
            }
        })
    }

    /// Poll the health endpoint until the server is ready.
    async fn wait_for_health(&self, url: &str) -> Result<()> {
        let health_url = format!("{}{}", url, self.health_check.path);
        let client = reqwest::Client::new();
        let timeout = Duration::from_millis(self.health_check.timeout_ms);
        let retry_delay = Duration::from_millis(self.health_check.retry_delay_ms);
        let start = std::time::Instant::now();

        loop {
            match client.get(&health_url).send().await {
                Ok(response) if response.status().is_success() => {
                    log::debug!("Health check passed for {}", url);
                    return Ok(());
                }
                Ok(response) => {
                    log::debug!(
                        "Health check returned status {}, retrying...",
                        response.status()
                    );
                }
                Err(e) => {
                    log::debug!("Health check failed: {}, retrying...", e);
                }
            }

            if start.elapsed() > timeout {
                return Err(
                    error_stack::report!(TransportError::Spawn).attach_printable(format!(
                        "Timeout waiting for health check at {}",
                        health_url
                    )),
                );
            }

            tokio::time::sleep(retry_delay).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use std::collections::HashMap;

    #[test]
    fn test_environment_variable_substitution() {
        // Test the subst functionality directly
        let env_vars: HashMap<String, String> = [
            ("HOME".to_string(), "/home/user".to_string()),
            ("USER".to_string(), "testuser".to_string()),
        ]
        .into_iter()
        .collect();

        let template = "Path: ${HOME}/documents";
        let result = subst::substitute(template, &env_vars).unwrap();
        assert_eq!(result, "Path: /home/user/documents");

        let template_with_default = "User: ${USER:-unknown}";
        let result = subst::substitute(template_with_default, &env_vars).unwrap();
        assert_eq!(result, "User: testuser");
    }

    #[test]
    fn test_launcher_env_substitution() {
        // Create a mock environment
        let mut test_env = HashMap::new();
        test_env.insert("TEST_HOME".to_string(), "/test/home".to_string());
        test_env.insert("TEST_USER".to_string(), "testuser".to_string());

        let mut env_config = IndexMap::new();
        env_config.insert("CUSTOM_HOME".to_string(), "${TEST_HOME}".to_string());
        env_config.insert("CUSTOM_USER".to_string(), "${TEST_USER}".to_string());
        env_config.insert(
            "CUSTOM_PATH".to_string(),
            "${TEST_HOME}/${TEST_USER}".to_string(),
        );

        // Test the substitution logic similar to what's in spawn()
        for (key, template) in &env_config {
            let substituted_value = subst::substitute(template, &test_env).unwrap();
            match key.as_str() {
                "CUSTOM_HOME" => assert_eq!(substituted_value, "/test/home"),
                "CUSTOM_USER" => assert_eq!(substituted_value, "testuser"),
                "CUSTOM_PATH" => assert_eq!(substituted_value, "/test/home/testuser"),
                _ => {}
            }
        }
    }
}
