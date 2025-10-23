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

#![allow(clippy::print_stderr)]

//! OTLP collector management for tracing integration tests

use std::path::{Path, PathBuf};
use testcontainers::{
    GenericImage, ImageExt as _,
    core::{IntoContainerPort as _, Mount, WaitFor},
    runners::AsyncRunner as _,
};

/// RAII guard for OTLP collector
///
/// Automatically stops the collector when dropped
pub struct CollectorGuard {
    _container: testcontainers::ContainerAsync<GenericImage>,
    pub endpoint: String,
    pub trace_dir: PathBuf,
}

impl CollectorGuard {
    /// Get the gRPC OTLP endpoint URL
    pub fn grpc_endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Get the trace file path
    pub fn trace_file(&self) -> PathBuf {
        self.trace_dir.join("traces.jsonl")
    }

    /// Get the log file path
    #[allow(dead_code)]
    pub fn log_file(&self) -> PathBuf {
        self.trace_dir.join("logs.jsonl")
    }
}

/// Start an OTLP collector for testing
///
/// Returns a guard that will stop the collector when dropped
pub async fn start_otlp_collector(test_name: &str) -> CollectorGuard {
    eprintln!("🔧 Starting OTLP collector for test: {}", test_name);

    // Ensure DOCKER_HOST is set for testcontainers
    if std::env::var("DOCKER_HOST").is_err() {
        let colima_socket = std::path::Path::new(&std::env::var("HOME").unwrap())
            .join(".colima/default/docker.sock");
        if colima_socket.exists() {
            // SAFETY: Setting env var during test initialization before any async work
            unsafe {
                std::env::set_var("DOCKER_HOST", format!("unix://{}", colima_socket.display()));
            }
        }
    }

    // Create temp directory for traces with test-specific name
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let temp_dir = std::path::PathBuf::from("target")
        .join("tracing-tests")
        .join(format!("{}-{}", test_name, timestamp));
    std::fs::create_dir_all(&temp_dir)
        .unwrap_or_else(|e| panic!("Failed to create trace directory: {}", e));
    let temp_dir_abs = std::fs::canonicalize(&temp_dir)
        .unwrap_or_else(|e| panic!("Failed to canonicalize temp directory: {}", e));

    // Get collector config path
    let config_path = get_collector_config_path();
    let config_path_abs = std::fs::canonicalize(&config_path).unwrap_or_else(|e| {
        panic!(
            "Failed to find otel-collector-config.yaml at {}: {}",
            config_path.display(),
            e
        )
    });

    // Start OTLP collector
    let start_time = std::time::Instant::now();
    let collector_image = GenericImage::new("otel/opentelemetry-collector", "latest")
        .with_exposed_port(4317.tcp())
        .with_exposed_port(4318.tcp())
        .with_wait_for(WaitFor::message_on_stderr("Everything is ready"))
        .with_mount(Mount::bind_mount(
            config_path_abs.to_str().unwrap(),
            "/etc/otel-collector-config.yaml",
        ))
        .with_mount(Mount::bind_mount(temp_dir_abs.to_str().unwrap(), "/output"))
        .with_cmd(vec!["--config=/etc/otel-collector-config.yaml"]);

    let collector = collector_image
        .start()
        .await
        .expect("Failed to start OTLP collector");

    let otlp_port = collector
        .get_host_port_ipv4(4317)
        .await
        .expect("Failed to get OTLP port");

    let endpoint = format!("http://localhost:{}", otlp_port);

    eprintln!(
        "✅ OTLP collector started on port {} in {:?}",
        otlp_port,
        start_time.elapsed()
    );

    // Give the collector time to fully initialize its gRPC endpoint
    eprintln!("⏳ Waiting for gRPC endpoint to be ready...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    CollectorGuard {
        _container: collector,
        endpoint,
        trace_dir: temp_dir_abs,
    }
}

/// Get the path to the collector config file
fn get_collector_config_path() -> PathBuf {
    // Tests run from the crate directory, so path is relative to that
    let config_path = Path::new("tests/tracing/otel-collector-config.yaml");

    if config_path.exists() {
        return config_path.to_path_buf();
    }

    // Fallback: try from CLI crate root
    let alt_path = Path::new("crates/stepflow-cli/tests/tracing/otel-collector-config.yaml");
    if alt_path.exists() {
        return alt_path.to_path_buf();
    }

    panic!(
        "Could not find otel-collector-config.yaml in expected locations:\n  - {}\n  - {}",
        config_path.display(),
        alt_path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Docker
    async fn test_start_collector() {
        let guard = start_otlp_collector("test_start_collector").await;
        assert!(guard.endpoint.starts_with("http://localhost:"));
        assert!(guard.trace_dir.exists());
    }
}
