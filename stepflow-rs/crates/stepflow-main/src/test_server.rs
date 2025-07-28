// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

//! Test server management for automated test execution.

use std::collections::HashMap;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use error_stack::ResultExt as _;
use serde_json::Value as JsonValue;
use stepflow_core::workflow::{TestServerConfig, TestServerHealthCheck};
use tokio::process::Command;

use crate::{MainError, Result};

/// Represents a running test server instance.
pub struct TestServer {
    /// Server name.
    pub name: String,
    /// Port the server is running on.
    pub port: u16,
    /// URL to access the server.
    pub url: String,
    /// Process handle for the server.
    process: tokio::process::Child,
}

impl TestServer {
    /// Stop the server.
    pub async fn stop(mut self) -> Result<()> {
        // Try graceful shutdown first
        #[cfg(unix)]
        {
            if let Some(id) = self.process.id() {
                use nix::sys::signal::{self, Signal};
                use nix::unistd::Pid;
                let _ = signal::kill(Pid::from_raw(id as i32), Signal::SIGTERM);
            }
        }

        // Wait for process to exit
        let timeout = Duration::from_secs(5);
        let start = Instant::now();

        while start.elapsed() < timeout {
            if let Ok(Some(_)) = self.process.try_wait() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Force kill if still running
        self.process.kill().await.ok();
        let _ = self.process.wait().await;
        Ok(())
    }
}

/// Manages a collection of test servers.
pub struct TestServerManager {
    servers: Vec<TestServer>,
    port_allocations: HashMap<String, u16>,
}

impl Default for TestServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TestServerManager {
    /// Create a new server manager.
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            port_allocations: HashMap::new(),
        }
    }

    /// Start all configured test servers.
    pub async fn start_servers(
        &mut self,
        server_configs: &HashMap<String, TestServerConfig>,
        working_directory: &std::path::Path,
    ) -> Result<()> {
        for (name, config) in server_configs {
            let server = self
                .start_server(name.clone(), config, working_directory)
                .await?;
            self.servers.push(server);
        }
        Ok(())
    }

    /// Start a single test server.
    async fn start_server(
        &mut self,
        name: String,
        config: &TestServerConfig,
        working_directory: &std::path::Path,
    ) -> Result<TestServer> {
        // Allocate port
        let port = self.allocate_port(&name, config)?;
        self.port_allocations.insert(name.clone(), port);

        // Prepare command with port substitution
        let mut command = Command::new(&config.command);

        // Substitute port in arguments
        let args: Vec<String> = config
            .args
            .iter()
            .map(|arg| substitute_placeholders(arg, &name, port))
            .collect();
        command.args(&args);

        // Set working directory
        let working_dir = if let Some(ref wd) = config.working_directory {
            working_directory.join(wd)
        } else {
            working_directory.to_owned()
        };
        command.current_dir(&working_dir);

        // Set environment variables with port substitution
        for (key, value) in &config.env {
            let substituted_value = substitute_placeholders(value, &name, port);
            command.env(key, substituted_value);
        }

        // Start the process
        let process = command
            .spawn()
            .change_context(MainError::FlowExecution)
            .attach_printable_lazy(|| format!("Failed to start test server '{name}"))?;

        let url = format!("http://127.0.0.1:{port}");
        let server = TestServer {
            name: name.clone(),
            port,
            url,
            process,
        };

        // Wait for server to become ready
        if let Some(health_check) = &config.health_check {
            self.wait_for_health_check(&server, health_check, config.startup_timeout_ms)
                .await?;
        } else {
            // Basic port check if no health check is configured
            self.wait_for_port(port, config.startup_timeout_ms).await?;
        }

        tracing::info!("✓ Started test server '{name}' on port {port}");
        Ok(server)
    }

    /// Allocate a port for a server.
    fn allocate_port(&self, _name: &str, config: &TestServerConfig) -> Result<u16> {
        if let Some((start, end)) = config.port_range {
            for port in start..=end {
                if is_port_available(port) {
                    return Ok(port);
                }
            }
            Err(MainError::FlowExecution).attach_printable("No available ports in specified range")
        } else {
            // Find any available port
            for port in 18000..19000 {
                if is_port_available(port) {
                    return Ok(port);
                }
            }
            Err(MainError::FlowExecution).attach_printable("No available ports found")
        }
    }

    /// Wait for a health check to pass.
    async fn wait_for_health_check(
        &self,
        server: &TestServer,
        health_check: &TestServerHealthCheck,
        timeout_ms: u64,
    ) -> Result<()> {
        let client = reqwest::Client::new();
        let health_url = format!("{}{}", server.url, health_check.path);
        let timeout = Duration::from_millis(timeout_ms);
        let start = Instant::now();

        for attempt in 1..=health_check.retry_attempts {
            if start.elapsed() > timeout {
                return Err(MainError::FlowExecution).attach_printable_lazy(|| {
                    format!("Health check timeout for server '{}'", server.name)
                });
            }

            let check_timeout = Duration::from_millis(health_check.timeout_ms);
            match tokio::time::timeout(check_timeout, client.get(&health_url).send()).await {
                Ok(Ok(response)) if response.status().is_success() => {
                    tracing::info!("✓ Health check passed for server '{}'", server.name);
                    return Ok(());
                }
                Ok(Ok(response)) => {
                    tracing::warn!(
                        "✗ Health check failed for server '{}': HTTP {}",
                        server.name,
                        response.status()
                    );
                }
                Ok(Err(e)) => {
                    tracing::warn!("✗ Health check error for server '{}': {}", server.name, e);
                }
                Err(_) => {
                    tracing::warn!(
                        "✗ Health check timeout for server '{}' (attempt {}/{})",
                        server.name,
                        attempt,
                        health_check.retry_attempts
                    );
                }
            }

            if attempt < health_check.retry_attempts {
                tokio::time::sleep(Duration::from_millis(health_check.retry_delay_ms)).await;
            }
        }

        Err(MainError::FlowExecution).attach_printable_lazy(|| {
            format!(
                "Health check failed for server '{}' after {} attempts",
                server.name, health_check.retry_attempts
            )
        })
    }

    /// Wait for a port to become available (basic check).
    async fn wait_for_port(&self, port: u16, timeout_ms: u64) -> Result<()> {
        let timeout = Duration::from_millis(timeout_ms);
        let start = Instant::now();

        while start.elapsed() < timeout {
            if !is_port_available(port) {
                // Port is now in use, assume server is ready
                tokio::time::sleep(Duration::from_millis(500)).await; // Give it a moment to fully start
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(MainError::FlowExecution)
            .attach_printable_lazy(|| format!("Server did not start on port {port} within timeout"))
    }

    /// Stop all running servers.
    pub async fn stop_all_servers(self) -> Result<()> {
        if !self.servers.is_empty() {
            tracing::info!("Stopping {} test servers...", self.servers.len());

            for server in self.servers {
                let server_name = server.name.clone();
                if let Err(e) = server.stop().await {
                    tracing::warn!("Failed to stop server '{server_name}': {e}");
                } else {
                    tracing::info!("✓ Stopped test server '{server_name}'");
                }
            }
        }

        Ok(())
    }

    /// Get the URL for a server by name.
    pub fn get_server_url(&self, name: &str) -> Option<&str> {
        self.servers
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.url.as_str())
    }

    /// Get the port for a server by name.
    pub fn get_server_port(&self, name: &str) -> Option<u16> {
        self.port_allocations.get(name).copied()
    }

    /// Create a config with server URL substitutions.
    pub fn create_substituted_config(&self, config: &JsonValue) -> Result<String> {
        let config_str = serde_json::to_string(config)
            .change_context(MainError::Configuration)
            .attach_printable("Failed to serialize test config")?;

        let mut result = config_str;

        // Substitute server URLs and ports
        for server in &self.servers {
            let url_placeholder = format!("{{{}.url}}", server.name);
            let port_placeholder = format!("{{{}.port}}", server.name);

            result = result.replace(&url_placeholder, &server.url);
            result = result.replace(&port_placeholder, &server.port.to_string());
        }

        Ok(result)
    }
}

/// Check if a port is available.
fn is_port_available(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_err()
}

/// Substitute placeholders in a string.
fn substitute_placeholders(input: &str, server_name: &str, port: u16) -> String {
    input
        .replace("{port}", &port.to_string())
        .replace(&format!("{{{server_name}.port}}"), &port.to_string())
        .replace(
            &format!("{{{server_name}.url}}"),
            &format!("http://127.0.0.1:{port}"),
        )
}
