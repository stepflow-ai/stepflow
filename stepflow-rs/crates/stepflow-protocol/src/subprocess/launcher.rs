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
}

/// Handle to a running subprocess.
pub struct SubprocessHandle {
    #[allow(dead_code)] // Kept for kill_on_drop behavior
    child: Child,
    url: String,
    stderr_task: tokio::task::JoinHandle<()>,
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
        // Abort the stderr monitoring task when the handle is dropped
        self.stderr_task.abort();

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

        // Read port from stdout
        let port = self.read_port_from_stdout(&mut child).await?;
        let url = format!("http://127.0.0.1:{}", port);

        // Start stderr monitoring
        let stderr_task = self.spawn_stderr_monitor(&mut child);

        // Wait for health check
        self.wait_for_health(&url).await?;

        log::info!("Subprocess HTTP server ready at {}", url);

        Ok(SubprocessHandle {
            child,
            url,
            stderr_task,
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

    /// Read the port announcement from the subprocess stdout.
    async fn read_port_from_stdout(&self, child: &mut Child) -> Result<u16> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| error_stack::report!(TransportError::Spawn))
            .attach_printable("Failed to get subprocess stdout")?;

        let mut reader = BufReader::new(stdout).lines();
        let timeout = Duration::from_secs(30);

        let result = tokio::time::timeout(timeout, async {
            // Read first line only - this should be the port announcement
            if let Some(line) = reader.next_line().await.change_context(TransportError::Spawn)? {
                let announcement: PortAnnouncement =
                    serde_json::from_str(&line).change_context(TransportError::Spawn).attach_printable_lazy(|| {
                        format!(
                            "Failed to parse port announcement. Expected JSON like {{\"port\": N}}, got: {}",
                            line
                        )
                    })?;
                log::info!("Subprocess announced port: {}", announcement.port);
                Ok(announcement.port)
            } else {
                Err(error_stack::report!(TransportError::Spawn)
                    .attach_printable("Subprocess closed stdout without announcing port"))
            }
        })
        .await;

        match result {
            Ok(Ok(port)) => Ok(port),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(error_stack::report!(TransportError::Spawn)
                .attach_printable("Timeout waiting for port announcement from subprocess")),
        }
    }

    /// Spawn a task to monitor and log stderr from the subprocess.
    fn spawn_stderr_monitor(&self, child: &mut Child) -> tokio::task::JoinHandle<()> {
        let stderr = child.stderr.take();
        let verbose = std::env::var("STEPFLOW_COMPONENT_STDERR")
            .map(|v| v.to_lowercase() == "verbose")
            .unwrap_or(false);

        tokio::spawn(async move {
            if let Some(stderr) = stderr {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if verbose {
                        log::info!("Component stderr: {}", line);
                    } else {
                        log::debug!("Component stderr: {}", line);
                    }
                }
            }
        })
    }

    /// Poll the health endpoint until the server is ready.
    async fn wait_for_health(&self, url: &str) -> Result<()> {
        let health_url = format!("{}/health", url);
        let client = reqwest::Client::new();
        let timeout = Duration::from_secs(60);
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

            tokio::time::sleep(Duration::from_millis(100)).await;
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
