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

//! Test that observability can be initialized without an existing Tokio runtime
//!
//! This is important for applications like Pingora that manage their own runtime.
//!
//! Note: These tests can only be run individually since they initialize the global logger.
//! Run with: cargo test -p stepflow-observability --test test_init_without_runtime -- --test-threads=1

use stepflow_observability::{
    BinaryObservabilityConfig, LogDestinationType, LogFormat, ObservabilityConfig,
    init_observability,
};

#[test]
fn test_init_observability_without_runtime_with_otlp() {
    // This test runs WITHOUT a Tokio runtime
    // It should succeed by creating a temporary runtime for OTLP initialization

    let config = ObservabilityConfig {
        log_level: log::LevelFilter::Info,
        other_log_level: None,
        log_destination: LogDestinationType::Stdout,
        log_format: LogFormat::Json,
        log_file: None,
        trace_enabled: true,  // Requires OTLP
        metrics_enabled: true,  // Requires OTLP
        otlp_endpoint: Some("http://localhost:4317".to_string()),
    };

    let binary_config = BinaryObservabilityConfig {
        service_name: "test-service",
        include_run_diagnostic: false,
    };

    // This should succeed by creating a temporary runtime
    // The key assertion is that this doesn't panic with "no reactor running"
    let guard = init_observability(&config, binary_config)
        .expect("Should initialize by creating temporary runtime for OTLP");

    // Verify we can log without errors
    log::info!("Test log from runtime init test");

    // Leak the guard to avoid panic on drop
    guard.leak();
}
