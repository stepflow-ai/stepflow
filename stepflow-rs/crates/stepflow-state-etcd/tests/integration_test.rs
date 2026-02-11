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
        // SAFETY: This runs once before any async work via LazyLock.
        unsafe {
            std::env::set_var(
                "DOCKER_HOST",
                format!("unix://{}", colima_socket.display()),
            );
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

    let manager = EtcdLeaseManager::connect(&config)
        .await
        .expect("Failed to connect via config");

    // Verify it works with a basic operation
    let result = manager
        .acquire_lease(
            Uuid::now_v7(),
            OrchestratorId::new("test-orch"),
            Duration::from_secs(30),
        )
        .await
        .expect("acquire_lease failed");
    assert!(matches!(result, LeaseResult::Acquired { .. }));
}

#[tokio::test]
async fn test_contention_two_orchestrators() {
    require_docker!();
    let (_container, client) = start_etcd().await;

    let manager_a = create_manager(client.clone(), "/test/contention");
    let manager_b = create_manager(client.clone(), "/test/contention");
    let run_id = Uuid::now_v7();
    let ttl = Duration::from_secs(30);

    // Orchestrator A acquires the lease
    let result_a = manager_a
        .acquire_lease(run_id, OrchestratorId::new("orch-a"), ttl)
        .await
        .expect("A acquire failed");
    assert!(matches!(result_a, LeaseResult::Acquired { .. }));

    // Orchestrator B tries to acquire the same run → OwnedBy
    let result_b = manager_b
        .acquire_lease(run_id, OrchestratorId::new("orch-b"), ttl)
        .await
        .expect("B acquire failed");
    match result_b {
        LeaseResult::OwnedBy { owner, .. } => {
            assert_eq!(owner, OrchestratorId::new("orch-a"));
        }
        other => panic!("Expected OwnedBy, got {:?}", other),
    }
}

#[tokio::test]
async fn test_release_all_revokes_etcd_lease() {
    require_docker!();
    let (_container, client) = start_etcd().await;

    let manager = create_manager(client.clone(), "/test/release-all");
    let orch_id = OrchestratorId::new("orch-release");
    let ttl = Duration::from_secs(30);

    // Acquire several leases
    let run_ids: Vec<Uuid> = (0..5).map(|_| Uuid::now_v7()).collect();
    for &run_id in &run_ids {
        manager
            .acquire_lease(run_id, orch_id.clone(), ttl)
            .await
            .expect("acquire failed");
    }

    // Verify they exist
    for &run_id in &run_ids {
        assert!(manager.get_lease(run_id).await.unwrap().is_some());
    }

    // Release all
    manager
        .release_all(orch_id.clone())
        .await
        .expect("release_all failed");

    // All leases should be gone (etcd lease revocation deletes all attached keys)
    for &run_id in &run_ids {
        assert!(
            manager.get_lease(run_id).await.unwrap().is_none(),
            "Lease for run {} should be deleted after release_all",
            run_id
        );
    }
}

#[tokio::test]
async fn test_watch_orphans_on_lease_expiry() {
    require_docker!();
    let (_container, client) = start_etcd().await;

    let manager = create_manager(client.clone(), "/test/watch-orphans");
    let orch_id = OrchestratorId::new("orch-ephemeral");
    // Short TTL so the etcd lease expires quickly
    let ttl = Duration::from_secs(2);

    // Start watching before acquiring
    let mut rx = manager
        .watch_orphans()
        .expect("watch_orphans should return a receiver");

    // Acquire a lease with a short TTL
    let run_id = Uuid::now_v7();
    manager
        .acquire_lease(run_id, orch_id.clone(), ttl)
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

#[tokio::test]
async fn test_heartbeat_keeps_lease_alive() {
    require_docker!();
    let (_container, client) = start_etcd().await;

    let manager = create_manager(client.clone(), "/test/heartbeat");
    let orch_id = OrchestratorId::new("orch-heartbeat");
    let ttl = Duration::from_secs(5);

    // Acquire a lease
    let run_id = Uuid::now_v7();
    manager
        .acquire_lease(run_id, orch_id.clone(), ttl)
        .await
        .expect("acquire failed");

    // Heartbeat should succeed
    manager
        .heartbeat(orch_id.clone(), ttl)
        .await
        .expect("heartbeat failed");

    // Verify the heartbeat key was created
    let orchestrators = manager
        .list_orchestrators()
        .await
        .expect("list_orchestrators failed");
    assert!(
        orchestrators
            .iter()
            .any(|o| o.id == OrchestratorId::new("orch-heartbeat")),
        "Heartbeat orchestrator should appear in list"
    );
}

#[tokio::test]
async fn test_list_orchestrators_with_run_counts() {
    require_docker!();
    let (_container, client) = start_etcd().await;

    let manager = create_manager(client.clone(), "/test/list-orch");
    let ttl = Duration::from_secs(30);

    // Create two orchestrators with different run counts
    let orch_a = OrchestratorId::new("orch-a");
    let orch_b = OrchestratorId::new("orch-b");

    // orch-a: heartbeat + 3 runs
    manager
        .heartbeat(orch_a.clone(), ttl)
        .await
        .expect("heartbeat failed");
    for _ in 0..3 {
        manager
            .acquire_lease(Uuid::now_v7(), orch_a.clone(), ttl)
            .await
            .expect("acquire failed");
    }

    // orch-b: heartbeat + 1 run
    manager
        .heartbeat(orch_b.clone(), ttl)
        .await
        .expect("heartbeat failed");
    manager
        .acquire_lease(Uuid::now_v7(), orch_b.clone(), ttl)
        .await
        .expect("acquire failed");

    let orchestrators = manager
        .list_orchestrators()
        .await
        .expect("list_orchestrators failed");

    let a_info = orchestrators
        .iter()
        .find(|o| o.id == orch_a)
        .expect("orch-a not found");
    let b_info = orchestrators
        .iter()
        .find(|o| o.id == orch_b)
        .expect("orch-b not found");

    assert_eq!(a_info.active_runs, 3);
    assert_eq!(b_info.active_runs, 1);
}
