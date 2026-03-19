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

//! Integration tests for NatsTaskTransport using testcontainers.
//!
//! These tests require Docker. When Docker is not available, tests are
//! automatically skipped (reported as passed with a skip message).

use std::collections::HashMap;

use stepflow_grpc::TaskAssignment;
use stepflow_grpc::task_transport::{TaskTransport, TaskTransportRead};
use stepflow_grpc::transport_compliance::TransportComplianceTests;
use stepflow_nats::NatsTaskTransport;
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

async fn create_transport(url: &str, default_stream: Option<String>) -> NatsTaskTransport {
    NatsTaskTransport::connect(url, default_stream)
        .await
        .expect("Failed to connect to NATS")
}

fn make_task(id: &str) -> TaskAssignment {
    TaskAssignment {
        task_id: id.to_string(),
        request: None,
        deadline_secs: 30,
        heartbeat_interval_secs: 1,
        execution_timeout_secs: 0,
    }
}

// =============================================================================
// Transport Compliance Suite
// =============================================================================

#[tokio::test]
async fn compliance_suite() {
    stepflow_test_utils::require_docker!();
    let (_container, url) = start_nats().await;

    // Each test gets a unique stream name to avoid cross-test interference.
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let url_clone = url.clone();
    TransportComplianceTests::run_all_readable(
        || {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let u = url_clone.clone();
            async move {
                let default_stream = format!("COMPLIANCE_{n}");
                let alt_stream = format!("COMPLIANCE_{n}_ALT");
                let transport = Box::new(create_transport(&u, Some(default_stream.clone())).await)
                    as Box<dyn TaskTransportRead>;
                (transport, default_stream, alt_stream)
            }
        },
        "stream", // route param key for NATS
    )
    .await;
}

// =============================================================================
// NATS-Specific Tests
// =============================================================================

/// Tasks published to a NATS stream can be consumed by a raw JetStream
/// subscriber (verifying wire format and stream creation).
#[tokio::test]
async fn test_publish_and_consume_raw_nats() {
    stepflow_test_utils::require_docker!();
    let (_container, url) = start_nats().await;

    let stream_name = "RAW_TEST";
    let transport = create_transport(&url, Some(stream_name.to_string())).await;

    let empty_params = HashMap::new();
    transport
        .send_task(make_task("raw-1"), &empty_params)
        .await
        .expect("send_task should succeed");

    // Consume via raw NATS client to verify wire format
    let client = async_nats::connect(&url).await.unwrap();
    let js = async_nats::jetstream::new(client);

    let stream = js
        .get_stream(stream_name)
        .await
        .expect("Stream should exist");

    let consumer = stream
        .get_or_create_consumer(
            "raw-consumer",
            async_nats::jetstream::consumer::pull::Config {
                filter_subject: format!("{stream_name}.tasks"),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    use futures::StreamExt;
    let mut messages = consumer.fetch().max_messages(1).messages().await.unwrap();
    let msg = messages.next().await.unwrap().unwrap();

    use prost::Message;
    let task = TaskAssignment::decode(msg.payload.as_ref()).unwrap();
    assert_eq!(task.task_id, "raw-1");
}

/// Protobuf fields survive encode/decode through NATS.
#[tokio::test]
async fn test_protobuf_field_preservation() {
    stepflow_test_utils::require_docker!();
    let (_container, url) = start_nats().await;

    let stream_name = "PROTO_TEST";
    let transport = create_transport(&url, Some(stream_name.to_string())).await;

    let original = TaskAssignment {
        task_id: "fields-1".to_string(),
        request: None,
        deadline_secs: 42,
        heartbeat_interval_secs: 2,
        execution_timeout_secs: 300,
    };

    let empty_params = HashMap::new();
    transport
        .send_task(original.clone(), &empty_params)
        .await
        .unwrap();

    let received = transport
        .recv_task(stream_name, std::time::Duration::from_secs(5))
        .await
        .unwrap()
        .expect("Should receive task");

    assert_eq!(received.task_id, original.task_id);
    assert_eq!(received.deadline_secs, original.deadline_secs);
    assert_eq!(
        received.heartbeat_interval_secs,
        original.heartbeat_interval_secs
    );
    assert_eq!(
        received.execution_timeout_secs,
        original.execution_timeout_secs
    );
}

/// Different streams via route params are fully independent work queues.
#[tokio::test]
async fn test_stream_isolation() {
    stepflow_test_utils::require_docker!();
    let (_container, url) = start_nats().await;

    let transport = create_transport(&url, Some("STREAM_A".to_string())).await;

    // Publish to two different streams
    let empty_params = HashMap::new();
    transport
        .send_task(make_task("a-task"), &empty_params)
        .await
        .unwrap();

    let mut params = HashMap::new();
    params.insert("stream".to_string(), serde_json::json!("STREAM_B"));
    transport
        .send_task(make_task("b-task"), &params)
        .await
        .unwrap();

    // Each stream should have exactly its own task
    let a = transport
        .recv_task("STREAM_A", std::time::Duration::from_secs(5))
        .await
        .unwrap()
        .expect("Should receive from STREAM_A");
    assert_eq!(a.task_id, "a-task");

    let b = transport
        .recv_task("STREAM_B", std::time::Duration::from_secs(5))
        .await
        .unwrap()
        .expect("Should receive from STREAM_B");
    assert_eq!(b.task_id, "b-task");
}
