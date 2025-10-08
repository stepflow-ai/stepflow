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

use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

static INIT_TEST_LOGGING: std::sync::Once = std::sync::Once::new();

/// Makes sure logging is initialized for test.
///
/// This needs to be called on each test.
pub fn init_test_logging() {
    INIT_TEST_LOGGING.call_once(|| {
        // We don't use a test writer for end to end tests.
        let fmt_layer = tracing_subscriber::fmt::layer();

        tracing_subscriber::registry()
            .with(EnvFilter::new("stepflow_=trace,info"))
            .with(fmt_layer)
            .with(tracing_error::ErrorLayer::default())
            .try_init()
            .unwrap();
    });
}

// Integration test functions moved to scripts/test-integration.sh
// These tests required external Python environment setup and are now run
// as part of the integration test suite to keep unit tests fast.
