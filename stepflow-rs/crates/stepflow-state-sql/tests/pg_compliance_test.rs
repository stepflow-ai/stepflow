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

#![cfg(feature = "postgres")]

//! PostgreSQL compliance tests using testcontainers.
//!
//! These tests verify that the unified SqlStateStore works correctly with
//! PostgreSQL, running the full compliance suites for all four store traits.
//!
//! Requires Docker. Automatically skipped locally when Docker is unavailable;
//! fails loudly in CI.

use stepflow_state::blob_compliance::BlobStoreComplianceTests;
use stepflow_state::checkpoint_compliance::CheckpointComplianceTests;
use stepflow_state::journal_compliance::JournalComplianceTests;
use stepflow_state::metadata_compliance::MetadataComplianceTests;
use stepflow_state_sql::{SqlStateStore, SqlStateStoreConfig};
use testcontainers::core::IntoContainerPort as _;
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner as _;
use testcontainers::{ContainerAsync, GenericImage, ImageExt as _};

/// Start a PostgreSQL container and return (container, connection_url).
async fn start_postgres() -> (ContainerAsync<GenericImage>, String) {
    let pg = GenericImage::new("postgres", "16-alpine")
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_exposed_port(5432.tcp())
        .with_env_var("POSTGRES_USER", "stepflow")
        .with_env_var("POSTGRES_PASSWORD", "stepflow")
        .with_env_var("POSTGRES_DB", "stepflow_test")
        .start()
        .await
        .expect("Failed to start PostgreSQL container");

    let port = pg
        .get_host_port_ipv4(5432)
        .await
        .expect("Failed to get PostgreSQL port");

    let url = format!("postgres://stepflow:stepflow@localhost:{port}/stepflow_test");

    // Wait a moment for Postgres to fully initialize
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    (pg, url)
}

/// Create a SqlStateStore connected to the given Postgres URL.
async fn create_pg_store(url: &str) -> SqlStateStore {
    SqlStateStore::new(SqlStateStoreConfig {
        database_url: url.to_string(),
        max_connections: 5,
        auto_migrate: true,
    })
    .await
    .expect("Failed to create PostgreSQL SqlStateStore")
}

// =============================================================================
// Blob Compliance
// =============================================================================

#[tokio::test]
async fn pg_blob_compliance() {
    stepflow_test_utils::require_docker!();
    let (_container, url) = start_postgres().await;

    BlobStoreComplianceTests::run_all_isolated(|| {
        let u = url.clone();
        async move { create_pg_store(&u).await }
    })
    .await;
}

// =============================================================================
// Metadata Compliance
// =============================================================================

#[tokio::test]
async fn pg_metadata_compliance() {
    stepflow_test_utils::require_docker!();
    let (_container, url) = start_postgres().await;

    MetadataComplianceTests::run_all_isolated(|| {
        let u = url.clone();
        async move { create_pg_store(&u).await }
    })
    .await;
}

// =============================================================================
// Journal Compliance
// =============================================================================

#[tokio::test]
async fn pg_journal_compliance() {
    stepflow_test_utils::require_docker!();
    let (_container, url) = start_postgres().await;

    JournalComplianceTests::run_all_isolated(|| {
        let u = url.clone();
        async move { create_pg_store(&u).await }
    })
    .await;
}

// =============================================================================
// Checkpoint Compliance
// =============================================================================

#[tokio::test]
async fn pg_checkpoint_compliance() {
    stepflow_test_utils::require_docker!();
    let (_container, url) = start_postgres().await;

    CheckpointComplianceTests::run_all_isolated(|| {
        let u = url.clone();
        async move { create_pg_store(&u).await }
    })
    .await;
}
