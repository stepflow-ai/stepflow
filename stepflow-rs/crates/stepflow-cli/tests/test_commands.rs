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

use stepflow_observability::{
    BinaryObservabilityConfig, LogDestinationType, LogFormat, ObservabilityConfig,
    init_observability,
};

static INIT_TEST_LOGGING: std::sync::Once = std::sync::Once::new();

/// Makes sure logging is initialized for test.
///
/// This needs to be called on each test.
pub fn init_test_logging() {
    INIT_TEST_LOGGING.call_once(|| {
        let config = ObservabilityConfig {
            log_level: log::LevelFilter::Trace,
            other_log_level: None,
            log_destination: LogDestinationType::Stdout,
            log_format: LogFormat::Text,
            log_file: None,
            trace_enabled: false,
            otlp_endpoint: None,
            metrics_enabled: false,
            service_instance_id: None,
        };

        let binary_config = BinaryObservabilityConfig {
            service_name: "stepflow-cli-tests",
            include_run_diagnostic: true,
        };

        let guard =
            init_observability(&config, binary_config).expect("Failed to initialize observability");
        // For tests, we'll just leak the guard to avoid the panic on drop
        // In tests, we don't care about flushing telemetry at the end
        guard.leak();
    });
}

// Integration test functions moved to scripts/test-integration.sh
// These tests required external Python environment setup and are now run
// as part of the integration test suite to keep unit tests fast.
