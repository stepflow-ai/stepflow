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

//! Docker availability helpers for testcontainers-based integration tests.
//!
//! # Usage
//!
//! ```ignore
//! use stepflow_test_utils::{docker_available, init_docker_host, require_docker};
//!
//! #[tokio::test]
//! async fn my_docker_test() {
//!     require_docker!();
//!     // ... start containers ...
//! }
//! ```

use std::sync::LazyLock;

/// Check whether Docker is available on this machine.
///
/// Cached after the first call so the check only runs once per test binary.
/// Runs `docker info` silently — returns `true` if the command exits
/// successfully.
pub fn docker_available() -> bool {
    static AVAILABLE: LazyLock<bool> = LazyLock::new(|| {
        std::process::Command::new("docker")
            .arg("info")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    });
    *AVAILABLE
}

/// Configure `DOCKER_HOST` for testcontainers, supporting Colima on macOS.
///
/// If `DOCKER_HOST` is already set, or if HOME is not set / the Colima
/// socket does not exist, this is a no-op.
///
/// Call this once at the start of any test that uses testcontainers.
/// It is safe to call multiple times (internally guarded by `LazyLock`).
pub fn init_docker_host() {
    static INIT: LazyLock<()> = LazyLock::new(|| {
        if std::env::var_os("DOCKER_HOST").is_some() {
            return;
        }
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let colima_socket = std::path::Path::new(&home).join(".colima/default/docker.sock");
        if colima_socket.exists() {
            // SAFETY: `set_var` is not thread-safe, but this runs inside a `LazyLock`
            // before any testcontainers work begins. Acceptable for test-only code.
            unsafe {
                std::env::set_var("DOCKER_HOST", format!("unix://{}", colima_socket.display()));
            }
        }
    });
    // Force the LazyLock to initialize.
    #[allow(clippy::let_unit_value)]
    let _ = *INIT;
}

/// Ensure Docker is available before running a test.
///
/// - **CI (`CI` env var set)**: panics so the test fails loudly.
/// - **Local development**: prints a skip message and returns early
///   (test counts as passed).
///
/// Also initializes `DOCKER_HOST` for Colima support.
///
/// # Usage
///
/// ```ignore
/// #[tokio::test]
/// async fn my_test() {
///     stepflow_test_utils::require_docker!();
///     // Docker is available — start containers
/// }
/// ```
#[macro_export]
macro_rules! require_docker {
    () => {
        $crate::init_docker_host();
        if !$crate::docker_available() {
            if std::env::var_os("CI").is_some() {
                panic!("Docker is required for this test but is not available in CI");
            }
            eprintln!("skipped (Docker not available)");
            return;
        }
    };
}
