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

//! Integration tests for NatsJournal using testcontainers.
//!
//! These tests require Docker. When Docker is not available, tests are
//! automatically skipped (reported as passed with a skip message).

use stepflow_nats::NatsJournal;
use stepflow_state::ExecutionJournal as _;
use stepflow_state::journal_compliance::JournalComplianceTests;
use testcontainers::core::IntoContainerPort as _;
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner as _;
use testcontainers::{ContainerAsync, GenericImage, ImageExt as _};

/// Start a NATS server container with JetStream enabled.
async fn start_nats() -> (ContainerAsync<GenericImage>, String) {
    let nats = GenericImage::new("nats", "2.11-alpine")
        .with_wait_for(WaitFor::message_on_stderr("Server is ready"))
        .with_exposed_port(4222.tcp())
        .with_cmd(["-js"])
        .start()
        .await
        .expect("Failed to start NATS container");

    let port = nats
        .get_host_port_ipv4(4222)
        .await
        .expect("Failed to get NATS port");

    let url = format!("nats://localhost:{port}");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    (nats, url)
}

// =============================================================================
// Journal Compliance Suite
// =============================================================================

#[tokio::test]
async fn journal_compliance_suite() {
    stepflow_test_utils::require_docker!();
    let (_container, url) = start_nats().await;

    // All tests share one stream. Each test uses unique root_run_ids (Uuid::now_v7)
    // so they are naturally isolated via subject filtering.
    let stream_name = "JOURNAL_COMPLIANCE".to_string();

    // Initialize the stream once.
    let journal = NatsJournal::connect_with_stream(&url, stream_name.clone())
        .await
        .expect("Failed to connect NatsJournal");
    journal
        .initialize_journal()
        .await
        .expect("Failed to initialize journal");

    // run_all_isolated creates a fresh NatsJournal handle per test, but they
    // all point to the same underlying stream. Isolation comes from unique
    // root_run_ids in the compliance tests.
    let url_clone = url.clone();
    JournalComplianceTests::run_all_isolated(|| {
        let u = url_clone.clone();
        let s = stream_name.clone();
        async move {
            NatsJournal::connect_with_stream(&u, s)
                .await
                .expect("Failed to connect NatsJournal")
        }
    })
    .await;
}
