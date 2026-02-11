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

//! Integration tests for EtcdLeaseManager using testcontainers.
//!
//! These tests require Docker. When Docker is not available, tests are
//! automatically skipped (reported as passed with a skip message).

use std::sync::LazyLock;
use std::time::Duration;

use stepflow_state::lease_compliance::LeaseComplianceTests;
use stepflow_state::{LeaseManager, LeaseResult, OrchestratorId};
use stepflow_state_etcd::{EtcdLeaseManager, EtcdLeaseManagerConfig};
use testcontainers::core::IntoContainerPort as _;
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner as _;
use testcontainers::{ContainerAsync, GenericImage, ImageExt as _};
use uuid::Uuid;

/// Check whether Docker is available on this machine.
///
/// Cached after the first call so the check only runs once per test binary.
static DOCKER_AVAILABLE: LazyLock<bool> = LazyLock::new(|| {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
});

/// Skip the current test if Docker is not available.
///
/// Prints a message so `cargo test` output shows why the test was skipped,
/// then returns early (test counts as passed).
macro_rules! require_docker {
    () => {
        if !*DOCKER_AVAILABLE {
            eprintln!("skipped (Docker not available)");
            return;
        }
    };
}

/// One-time initialization to configure DOCKER_HOST for testcontainers.
///
/// Primarily supports Colima on macOS. If DOCKER_HOST is already set,
/// or if HOME is not set / the Colima socket does not exist, this is a no-op.
static INIT_DOCKER_HOST: LazyLock<()> = LazyLock::new(|| {
    if std::env::var_os("DOCKER_HOST").is_some() {
        return;
    }
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let colima_socket = std::path::Path::new(&home).join(".colima/default/docker.sock");
    if colima_socket.exists() {
        // SAFETY: `set_var` is not thread-safe, but this runs inside a `LazyLock`
        // that is forced (`let _ = *INIT_DOCKER_HOST`) at the top of `start_etcd()`
        // before any etcd client / testcontainers work begins. The tokio runtime
        // may already have worker threads, but none of them read `DOCKER_HOST`
        // before this point because `start_etcd()` is the first thing every test
        // calls. Acceptable trade-off for test-only code.
        unsafe {
            std::env::set_var("DOCKER_HOST", format!("unix://{}", colima_socket.display()));
        }
    }
});

/// Start an etcd container and return (container, client).
async fn start_etcd() -> (ContainerAsync<GenericImage>, etcd_client::Client) {
    // Ensure DOCKER_HOST is configured once in a thread-safe way
    let _ = *INIT_DOCKER_HOST;

    let etcd = GenericImage::new("gcr.io/etcd-development/etcd", "v3.5.21")
        .with_wait_for(WaitFor::message_on_stderr("ready to serve client requests"))
        .with_exposed_port(2379.tcp())
        .with_env_var("ETCD_LISTEN_CLIENT_URLS", "http://0.0.0.0:2379")
        .with_env_var("ETCD_ADVERTISE_CLIENT_URLS", "http://0.0.0.0:2379")
        .start()
        .await
        .expect("Failed to start etcd container");

    let port = etcd
        .get_host_port_ipv4(2379)
        .await
        .expect("Failed to get etcd port");

    // Retry connection — the container port may not be ready immediately
    // even after the wait condition is met.
    let endpoint = format!("http://localhost:{port}");
    let mut client = None;
    for _ in 0..10 {
        match etcd_client::Client::connect([&endpoint], None).await {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
    let client = client.expect("Failed to connect to etcd after retries");

    (etcd, client)
}

/// Create an EtcdLeaseManager connected to the test etcd instance.
fn create_manager(client: etcd_client::Client, prefix: &str) -> EtcdLeaseManager {
    EtcdLeaseManager::new(client, prefix.to_string())
}

// =============================================================================
// Compliance Suite
// =============================================================================

#[tokio::test]
async fn compliance_suite_isolated() {
    require_docker!();
    let (_container, client) = start_etcd().await;

    // Each test gets its own key prefix for isolation
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let client_clone = client.clone();

    LeaseComplianceTests::run_all_isolated(|| {
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let c = client_clone.clone();
        async move { create_manager(c, &format!("/test/compliance/{n}")) }
    })
    .await;
}

// =============================================================================
// etcd-Specific Tests
// =============================================================================

#[tokio::test]
async fn test_connect_from_config() {
    require_docker!();
    let (_container, _client) = start_etcd().await;

    let port = _container
        .get_host_port_ipv4(2379)
        .await
        .expect("Failed to get port");

    let config = EtcdLeaseManagerConfig {
        endpoints: vec![format!("http://localhost:{port}")],
        key_prefix: "/test/connect".to_string(),
    };

    let manager = EtcdLeaseManager::connect(&config, Duration::from_secs(30))
        .await
        .expect("Failed to connect via config");

    // Verify it works with a basic operation
    let result = manager
        .acquire_lease(Uuid::now_v7(), OrchestratorId::new("test-orch"))
        .await
        .expect("acquire_lease failed");
    assert!(matches!(result, LeaseResult::Acquired { .. }));
}

#[tokio::test]
async fn test_watch_orphans_on_lease_expiry() {
    require_docker!();
    let (_container, client) = start_etcd().await;

    // Use a short TTL so the etcd lease expires quickly
    let manager = EtcdLeaseManager::new_with_ttl(
        client.clone(),
        "/test/watch-orphans".to_string(),
        Duration::from_secs(2),
    );
    let orch_id = OrchestratorId::new("orch-ephemeral");

    // Start watching before acquiring
    let mut rx = manager
        .watch_orphans()
        .expect("watch_orphans should return a receiver");

    // Acquire a lease (using the short TTL configured on the manager)
    let run_id = Uuid::now_v7();
    manager
        .acquire_lease(run_id, orch_id.clone())
        .await
        .expect("acquire failed");

    // Release all to simulate crash (revokes the etcd lease immediately)
    manager
        .release_all(orch_id)
        .await
        .expect("release_all failed");

    // The watch should detect the DELETE event
    let orphaned_run_id = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("Timed out waiting for orphan notification")
        .expect("Channel closed");

    assert_eq!(orphaned_run_id, run_id);
}

